import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

export function useFocusOnNavigate(ref: React.RefObject<HTMLElement | null>) {
  const location = useLocation();

  useEffect(() => {
    if (ref.current) {
      ref.current.focus();
    }
  }, [location.pathname, ref]);
}
