import { useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { useClientExtensions } from './ClientExtensionsContext';
import { parseClientExtensionViewPath } from './routes';

export function ClientExtensionRegistrySync() {
  const location = useLocation();
  const navigate = useNavigate();
  const { extensions, loading } = useClientExtensions();

  useEffect(() => {
    if (loading) {
      return;
    }

    const view = parseClientExtensionViewPath(location.pathname);
    if (!view) {
      return;
    }

    const extension = extensions.find((entry) => entry.id === view.extensionId);
    if (!extension?.enabled) {
      navigate('/pair', { replace: true });
    }
  }, [extensions, loading, location.pathname, navigate]);

  return null;
}
