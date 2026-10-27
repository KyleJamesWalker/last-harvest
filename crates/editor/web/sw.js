// The card editor's offline copy. Network first, so a new release shows at once; the cache only stands in offline.
const CACHE = "last-harvest-card-editor";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET" || new URL(e.request.url).origin !== location.origin) return;
  e.respondWith(
    fetch(e.request)
      .then((response) => {
        if (response.status === 200) {
          const copy = response.clone();
          caches.open(CACHE).then((c) => c.put(e.request, copy)).catch(() => {});
        }
        return response;
      })
      .catch(() => caches.match(e.request, { ignoreSearch: true })),
  );
});
