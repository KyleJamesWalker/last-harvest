// The card editor's offline copy once lived at the site's root and now lives in editor/. A copy installed from
// then still runs this worker, so it drops the old root files from the cache and unregisters itself.
const CACHE = "last-harvest-card-editor";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) =>
  e.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      for (const request of await cache.keys()) {
        if (!new URL(request.url).pathname.includes("/editor/")) await cache.delete(request);
      }
      await self.registration.unregister();
    })(),
  ),
);
