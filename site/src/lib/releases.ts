// Where the app is published, and which file a visitor should get.
//
// The page links to the releases page by default, so the buttons work without
// JavaScript. The client script then swaps in the exact asset once it knows the
// visitor's platform and the names of the newest release.

export const REPO = "bilals2008/b-download-manager";
export const RELEASES_URL = `https://github.com/${REPO}/releases/latest`;

const API_URL = `https://api.github.com/repos/${REPO}/releases/latest`;
const CACHE_KEY = "bdm-latest-assets";

export type Platform = "windows" | "macos" | "linux" | "other";

export interface ReleaseAsset {
  name: string;
  url: string;
}

export const platformNames: Record<Exclude<Platform, "other">, string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

/// The visitor's desktop OS. Phones and tablets are "other": there is no build.
export function detectPlatform(): Platform {
  const navigatorData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const hint = navigatorData.userAgentData?.platform || navigator.platform || "";
  const agent = navigator.userAgent;

  if (/android|iphone|ipad|ipod/i.test(agent)) return "other";
  if (/^win/i.test(hint) || /windows/i.test(agent)) return "windows";
  // "Darwin" also contains "win", so the platform hint is matched from the start.
  if (/^mac/i.test(hint) || /macintosh/i.test(agent)) return "macos";
  if (/linux|x11/i.test(hint) || /linux/i.test(agent)) return "linux";
  return "other";
}

/// The release file for a platform, preferring the friendliest installer.
export function pickAsset(
  assets: ReleaseAsset[],
  platform: Platform,
): ReleaseAsset | undefined {
  const files = assets.filter((asset) => !asset.name.endsWith(".sig"));
  const match = (pattern: RegExp) => files.find((asset) => pattern.test(asset.name));

  if (platform === "windows") return match(/-setup\.exe$/i) ?? match(/\.exe$/i);
  if (platform === "linux") return match(/\.appimage$/i) ?? match(/\.deb$/i);
  if (platform === "macos") return match(/\.dmg$/i);
  return undefined;
}

/// The assets of the newest published release, cached for the browser session.
///
/// The GitHub API allows cross-origin reads but rate-limits them per address,
/// so the answer is reused instead of fetched on every page view.
export async function latestAssets(): Promise<ReleaseAsset[] | undefined> {
  try {
    const cached = sessionStorage.getItem(CACHE_KEY);
    if (cached) return JSON.parse(cached) as ReleaseAsset[];
  } catch {
    // A blocked sessionStorage costs the cache, not the download.
  }

  try {
    const response = await fetch(API_URL, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) return undefined;

    const release = (await response.json()) as {
      assets?: { name: string; browser_download_url: string }[];
    };
    const assets = (release.assets ?? []).map((asset) => ({
      name: asset.name,
      url: asset.browser_download_url,
    }));

    try {
      sessionStorage.setItem(CACHE_KEY, JSON.stringify(assets));
    } catch {
      // Same: caching is only an optimisation.
    }

    return assets;
  } catch {
    // Offline or blocked: the buttons keep the releases page.
    return undefined;
  }
}
