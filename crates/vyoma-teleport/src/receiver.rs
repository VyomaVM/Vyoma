use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tracing::{error, info};

use async_compression::tokio::write::ZstdDecoder;
use tonic::{Request, Response, Status, Streaming};

use vyoma_proto::teleport::v1::teleport_chunk::Content;
use vyoma_proto::teleport::v1::{TeleportAck, TeleportChunk};

pub struct TeleportReceiver {
    session_id: String,
    memory_file: PathBuf,
    state_file: PathBuf,
}

impl TeleportReceiver {
    pub fn new(memory_file: PathBuf, state_file: PathBuf) -> Self {
        Self {
            session_id: String::new(),
            memory_file,
            state_file,
        }
    }

    pub async fn process_stream(
        &mut self,
        request: Request<Streaming<TeleportChunk>>,
    ) -> Result<Response<tokio_stream::wrappers::ReceiverStream<Result<TeleportAck, Status>>>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        let mem_path = self.memory_file.clone();
        let state_path = self.state_file.clone();

        tokio::spawn(async move {
            let mut state_writer: Option<BufWriter<File>> = None;
            let mut mem_decoder: Option<ZstdDecoder<BufWriter<File>>> = None;

            while let Ok(Some(chunk)) = in_stream.message().await {
                let seq = chunk.chunk_sequence;
                let session = chunk.session_id;

                let mut ack = TeleportAck {
                    session_id: session.clone(),
                    processed_sequence: seq,
                    received: true,
                    error_msg: "".to_string(),
                };

                if let Some(content) = chunk.content {
                    match content {
                        Content::Metadata(meta) => {
                            info!("Initializing reception for VM {} (Session {})", meta.id, session);
                            
                            // Prepare state file
                            match OpenOptions::new().create(true).write(true).open(&state_path).await {
                                Ok(f) => state_writer = Some(BufWriter::new(f)),
                                Err(e) => {
                                    ack.received = false;
                                    ack.error_msg = format!("Failed to create state file: {}", e);
                                    let _ = tx.send(Ok(ack)).await;
                                    return;
                                }
                            }

                            // Prepare memory file with ZSTD decompression pipeline
                            match OpenOptions::new().create(true).write(true).open(&mem_path).await {
                                Ok(f) => {
                                    let buf_w = BufWriter::new(f);
                                    mem_decoder = Some(ZstdDecoder::new(buf_w));
                                }
                                Err(e) => {
                                    ack.received = false;
                                    ack.error_msg = format!("Failed to create memory file: {}", e);
                                    let _ = tx.send(Ok(ack)).await;
                                    return;
                                }
                            }
                        }
                        Content::StateChunk(data) => {
                            if let Some(w) = state_writer.as_mut() {
                                if let Err(e) = w.write_all(&data).await {
                                    ack.received = false;
                                    ack.error_msg = format!("State write failed: {}", e);
                                    let _ = tx.send(Ok(ack)).await;
                                    return;
                                }
                            }
                        }
                        Content::MemoryChunk(data) => {
                            if let Some(dec) = mem_decoder.as_mut() {
                                if let Err(e) = dec.write_all(&data).await {
                                    ack.received = false;
                                    ack.error_msg = format!("Memory chunk decomp/write failed: {}", e);
                                    let _ = tx.send(Ok(ack)).await;
                                    return;
                                }
                            }
                        }
                    }
                }

                if tx.send(Ok(ack)).await.is_err() {
                    error!("Ack stream broken");
                    break;
                }
            }
            
            // Flush cleanly
            if let Some(mut w) = state_writer {
                let _ = w.flush().await;
            }
            if let Some(mut d) = mem_decoder {
                let _ = d.shutdown().await;
            }

            info!("Teleportation stream processing completed cleanly");
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}
