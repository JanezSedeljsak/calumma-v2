import { useEffect, useState } from "react";

const REPO = "JanezSedeljsak/calumma-v2";
// One request serves both views, which matters: the API is unauthenticated and allows 60
// requests per hour per IP, and a visitor who opens both pages should still only spend one.
const RELEASES_URL = `https://api.github.com/repos/${REPO}/releases?per_page=20`;

type Asset = { name: string; browser_download_url: string; size: number };
type Release = {
  tag_name: string;
  name: string;
  html_url: string;
  published_at: string;
  draft: boolean;
  prerelease: boolean;
  assets: Asset[];
};

/// A platform is offered when a release actually carries an installer for it, and says
/// "coming soon" when it does not. Nothing here hardcodes which platforms have shipped — the
/// day a `.msi` or an `.AppImage` lands in a release, that card becomes a download on its own.
const PLATFORMS = [
  {
    id: "macos",
    name: "macOS",
    note: "Apple silicon, macOS 26 or later.",
    // `.sha256` sits beside every installer in the release; it is not one.
    match: (n: string) => n.endsWith(".dmg"),
    icon: (
      <path d="M16.4 12.7c0-2.2 1.8-3.3 1.9-3.4-1-1.5-2.6-1.7-3.2-1.7-1.4-.1-2.7.8-3.3.8-.7 0-1.7-.8-2.8-.8-1.5 0-2.8.8-3.6 2.1-1.5 2.6-.4 6.5 1.1 8.6.7 1 1.6 2.2 2.7 2.2 1.1 0 1.5-.7 2.8-.7s1.7.7 2.8.7c1.2 0 1.9-1.1 2.6-2.1.8-1.2 1.2-2.4 1.2-2.4-.1 0-2.2-.9-2.2-3.3zM14.2 6.2c.6-.7 1-1.7.9-2.7-.9 0-2 .6-2.6 1.3-.6.6-1.1 1.7-.9 2.6 1 .1 2-.5 2.6-1.2z" />
    ),
  },
  {
    id: "windows",
    name: "Windows",
    note: "Windows 11 or later, 64-bit.",
    match: (n: string) => n.endsWith(".msi") || n.endsWith(".exe"),
    icon: (
      <path d="M3 5.4 10.3 4.4v6.9H3V5.4zm0 13.2 7.3 1v-6.8H3v5.8zM11.4 4.2 21 3v8.3h-9.6V4.2zm0 8.2H21V21l-9.6-1.3v-7.3z" />
    ),
  },
  {
    id: "linux",
    name: "Linux",
    note: "x86_64, distribution-independent.",
    match: (n: string) =>
      n.endsWith(".AppImage") || n.endsWith(".deb") || n.endsWith(".rpm"),
    icon: (
      <path d="M12 2c-2.5 0-4 1.9-4 4.3 0 1.4.2 2.3.2 3.2 0 1-1.1 2.2-1.9 3.6-.8 1.4-1.6 2.9-1.6 4.4 0 1.1.5 1.9 1.3 2.4.9.5 2 .6 3 .5.5-.1.9-.3 1.3-.5.5-.3 1-.4 1.7-.4s1.2.1 1.7.4c.4.2.8.4 1.3.5 1 .1 2.1 0 3-.5.8-.5 1.3-1.3 1.3-2.4 0-1.5-.8-3-1.6-4.4-.8-1.4-1.9-2.6-1.9-3.6 0-.9.2-1.8.2-3.2C16 3.9 14.5 2 12 2zm-1.7 3.1c.5 0 .9.6.9 1.3s-.4 1.3-.9 1.3-.9-.6-.9-1.3.4-1.3.9-1.3zm3.4 0c.5 0 .9.6.9 1.3s-.4 1.3-.9 1.3-.9-.6-.9-1.3.4-1.3.9-1.3zM12 8.6c.9 0 1.9.4 1.9.9 0 .3-.3.5-.7.8-.4.3-.8.5-1.2.5s-.8-.2-1.2-.5c-.4-.3-.7-.5-.7-.8 0-.5 1-.9 1.9-.9z" />
    ),
  },
] as const;

type Route = "download" | "releases";

const routeOf = (hash: string): Route =>
  hash === "#/releases" ? "releases" : "download";

/// Hash routing rather than paths: Pages serves static files, so `/releases` would 404 on a
/// hard refresh unless the site shipped a catch-all. A hash never leaves the one document.
function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => routeOf(location.hash));
  useEffect(() => {
    const sync = () => setRoute(routeOf(location.hash));
    addEventListener("hashchange", sync);
    return () => removeEventListener("hashchange", sync);
  }, []);
  return route;
}

const formatSize = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;

const formatDate = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });

const installers = (release: Release) =>
  release.assets.filter((a) => PLATFORMS.some((p) => p.match(a.name)));

export default function App() {
  const route = useRoute();
  const [releases, setReleases] = useState<Release[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    fetch(RELEASES_URL, { headers: { Accept: "application/vnd.github+json" } })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((all: Release[]) => setReleases(all.filter((r) => !r.draft)))
      .catch(() => setFailed(true));
  }, []);

  // What the download cards point at: the newest real release, falling back to a prerelease
  // when that is all there has ever been.
  const latest =
    releases?.find((r) => !r.prerelease) ?? releases?.[0] ?? null;

  return (
    <div className="page">
      <header>
        <h1 className="brand">Calumma</h1>
        <p className="tagline">Your personal whiteboard</p>
        <p className="lede">
          Bounded project canvases you draw on with a pen or shapes. Multi-layer
          documents, live text, vectors and raster side by side, with the whole
          engine written to stay fast at scale.
        </p>
        <nav className="nav">
          <a className={route === "download" ? "active" : ""} href="#/">
            Download
          </a>
          <a className={route === "releases" ? "active" : ""} href="#/releases">
            Releases
          </a>
        </nav>
      </header>

      {route === "download" ? (
        <DownloadView latest={latest} failed={failed} />
      ) : (
        <ReleasesView releases={releases} failed={failed} />
      )}

      <footer className="footer">
        <span>
          {failed
            ? "Could not reach GitHub — grab the installers from the releases page."
            : "Installers are served straight from GitHub Releases."}
        </span>
        <span>
          <a href={`https://github.com/${REPO}/releases`}>All releases</a>
          {" · "}
          <a href={`https://github.com/${REPO}`}>Source</a>
        </span>
      </footer>
    </div>
  );
}

function DownloadView({
  latest,
  failed,
}: {
  latest: Release | null;
  failed: boolean;
}) {
  const version = latest?.tag_name.replace(/^v/, "");
  return (
    <section>
      <div className="section-head">
        <span className="dot" />
        <span className="label">
          Download{version ? ` · version ${version}` : ""}
        </span>
      </div>

      <div className="grid">
        {PLATFORMS.map((platform) => {
          const asset = latest?.assets.find((a) => platform.match(a.name));
          return (
            <article className="card" key={platform.id}>
              <div className="card-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="currentColor">
                  {platform.icon}
                </svg>
              </div>
              <h2 className="card-name">{platform.name}</h2>
              <p className="card-note">{platform.note}</p>
              {asset ? (
                <>
                  <a className="button primary" href={asset.browser_download_url}>
                    Download · {formatSize(asset.size)}
                  </a>
                  <span className="checksum">
                    <a href={`${asset.browser_download_url}.sha256`}>
                      {asset.name}.sha256
                    </a>
                  </span>
                </>
              ) : (
                <span className="button pending">
                  {failed ? "Releases unavailable" : "Coming soon"}
                </span>
              )}
            </article>
          );
        })}
      </div>
    </section>
  );
}

function ReleasesView({
  releases,
  failed,
}: {
  releases: Release[] | null;
  failed: boolean;
}) {
  return (
    <section>
      <div className="section-head">
        <span className="dot orange" />
        <span className="label">Releases</span>
      </div>

      {failed && <p className="card-note">Could not reach GitHub.</p>}
      {!failed && !releases && <p className="card-note">Loading…</p>}
      {releases?.length === 0 && <p className="card-note">No releases yet.</p>}

      <div className="release-list">
        {releases?.map((release) => (
          <article className="card release" key={release.tag_name}>
            <div className="release-head">
              <h2 className="card-name">{release.name || release.tag_name}</h2>
              {release.prerelease && <span className="badge">Prerelease</span>}
              <span className="release-date">
                {formatDate(release.published_at)}
              </span>
            </div>
            <div className="release-assets">
              {installers(release).map((asset) => (
                <a
                  className="button primary"
                  key={asset.name}
                  href={asset.browser_download_url}
                >
                  {asset.name} · {formatSize(asset.size)}
                </a>
              ))}
              <a className="button" href={release.html_url}>
                Release notes
              </a>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
