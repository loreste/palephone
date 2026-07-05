interface ServiceWorkerGateOptions {
  dev?: boolean;
  hasWindow?: boolean;
  hasNavigator?: boolean;
  hasServiceWorker?: boolean;
  isTauri?: boolean;
  secureContext?: boolean;
  hostname?: string;
}

export function shouldRegisterServiceWorker(options: ServiceWorkerGateOptions = {}): boolean {
  const hasWindow = options.hasWindow ?? typeof window !== "undefined";
  const hasNavigator = options.hasNavigator ?? typeof navigator !== "undefined";
  if (options.dev ?? import.meta.env.DEV) return false;
  if (!hasWindow || !hasNavigator) return false;
  const hasServiceWorker = options.hasServiceWorker ?? ("serviceWorker" in navigator);
  if (!hasServiceWorker) return false;
  const isTauri = options.isTauri ?? Boolean((window as any).__TAURI_INTERNALS__);
  if (isTauri) return false;
  const secureContext = options.secureContext ?? window.isSecureContext;
  const hostname = options.hostname ?? window.location.hostname;
  return secureContext || hostname === "localhost";
}

export function registerPaleServiceWorker(): Promise<ServiceWorkerRegistration | null> {
  if (!shouldRegisterServiceWorker()) {
    return Promise.resolve(null);
  }
  return navigator.serviceWorker.register("/sw.js", { scope: "/" });
}
