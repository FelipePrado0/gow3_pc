//! Which release is which: the tag's identity, the list GitHub returns and the newer-stable rule.

use serde_json::Value;

use crate::core::i18n::tr;
use crate::core::update::{UpdateError, RELEASES_PAGE};

/// The stamp's file name inside the package: `port_dir/release.json`, `$APPDIR/release.json`.
pub const STAMP_FILE: &str = "release.json";

/// The asset a release carries once this system's package in it is stamped. One per system: a
/// release often holds both packages, and each workflow stamps only its own.
pub fn stamp_asset(windows: bool) -> &'static str {
    if windows {
        "release-windows.json"
    } else {
        "release-linux.json"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Release,
    PreRelease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseId {
    pub channel: Channel,
    pub version: String,
    pub build: Option<String>,
}

impl ReleaseId {
    /// `<system>-v<version>[-<build>]`; the channel is the API's `prerelease`. None for a tag
    /// without `-v<digit>` or with text after the version that does not start with `-`.
    pub fn parse(tag: &str, prerelease: bool) -> Option<ReleaseId> {
        let start = tag
            .match_indices("-v")
            .map(|(at, _)| at + 2)
            .find(|&at| tag[at..].starts_with(|c: char| c.is_ascii_digit()))?;
        let rest = &tag[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let version = &rest[..end];
        let build = match &rest[end..] {
            "" => None,
            tail => Some(
                tail.strip_prefix('-')
                    .filter(|b| !b.is_empty())?
                    .to_string(),
            ),
        };
        Some(ReleaseId {
            channel: if prerelease {
                Channel::PreRelease
            } else {
                Channel::Release
            },
            version: version.to_string(),
            build,
        })
    }

    /// "Release · 2.6.0", "Pre-release · 2.6.0 · issue209-test1" (channel names through `tr`).
    pub fn label(&self, lang: &str) -> String {
        let channel = match self.channel {
            Channel::Release => tr(lang, "Release"),
            Channel::PreRelease => tr(lang, "Pre-release"),
        };
        format!("{channel} · {}", self.short())
    }

    /// The label without the channel: "2.6.0", "2.6.0 · issue209-test1".
    pub fn short(&self) -> String {
        match &self.build {
            Some(build) => format!("{} · {build}", self.version),
            None => self.version.clone(),
        }
    }

    /// Whether `other` is the same package: same version and build, whatever the channel. The
    /// stamp's channel is frozen when the package is built, while the list's follows the API's
    /// `prerelease`, which changes when a release is promoted or demoted.
    pub fn same_build(&self, other: &ReleaseId) -> bool {
        self.version == other.version && self.build == other.build
    }

    /// `Release · VERSION`, no build.
    pub fn fallback() -> ReleaseId {
        ReleaseId {
            channel: Channel::Release,
            version: crate::VERSION.to_string(),
            build: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub id: ReleaseId,
    pub published: String,
    pub url: String,
    pub page: String,
}

/// The first asset of `assets` whose name satisfies `wanted`: its download URL.
fn asset_url(assets: &[Value], wanted: impl Fn(&str) -> bool) -> Option<String> {
    assets.iter().find_map(|asset| {
        let name = asset.get("name")?.as_str()?;
        if !wanted(name) {
            return None;
        }
        Some(asset.get("browser_download_url")?.as_str()?.to_string())
    })
}

fn parse_entry(entry: &Value, windows: bool) -> Option<Release> {
    if entry.get("draft").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let prerelease = entry
        .get("prerelease")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let id = ReleaseId::parse(entry.get("tag_name")?.as_str()?, prerelease)?;
    let published = entry.get("published_at")?.as_str()?.to_string();
    let assets = entry.get("assets")?.as_array()?;
    asset_url(assets, |name| name == stamp_asset(windows))?;
    let suffix = if windows { ".zip" } else { ".appimage" };
    let url = asset_url(assets, |name| name.to_lowercase().ends_with(suffix))?;
    let page = entry
        .get("html_url")
        .and_then(Value::as_str)
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    Some(Release {
        id,
        published,
        url,
        page,
    })
}

/// The releases of this system, newest first. Err(UpdateError::Json) when `json` is not an array.
pub fn parse_releases(json: &Value, windows: bool) -> Result<Vec<Release>, UpdateError> {
    let entries = json
        .as_array()
        .ok_or_else(|| UpdateError::Json("the release list is not a JSON array".into()))?;
    let mut releases: Vec<Release> = entries
        .iter()
        .filter_map(|entry| parse_entry(entry, windows))
        .collect();
    releases.sort_by(|a, b| b.published.cmp(&a.published));
    Ok(releases)
}

/// The three newest stables, in list order.
pub fn stables(releases: &[Release]) -> Vec<&Release> {
    releases
        .iter()
        .filter(|r| r.id.channel == Channel::Release)
        .take(3)
        .collect()
}

/// Every pre-release, in list order.
pub fn prereleases(releases: &[Release]) -> Vec<&Release> {
    releases
        .iter()
        .filter(|r| r.id.channel == Channel::PreRelease)
        .collect()
}

/// The first stable whose version is above the installed one.
pub fn newer_stable<'a>(releases: &'a [Release], installed: &ReleaseId) -> Option<&'a Release> {
    let installed = version_tuple(&installed.version);
    releases
        .iter()
        .find(|r| r.id.channel == Channel::Release && version_tuple(&r.id.version) > installed)
}

/// Every run of digits in `text`, as numbers.
pub fn version_tuple(text: &str) -> Vec<u64> {
    digit_runs(text)
        .into_iter()
        .map(|run| run.parse().unwrap_or(u64::MAX))
        .collect()
}

pub(crate) fn digit_runs(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn asset(name: &str) -> Value {
        json!({"name": name, "browser_download_url": format!("https://x.test/{name}")})
    }
    fn entry(tag: &str, prerelease: bool, published: &str, assets: &[&str]) -> Value {
        json!({"tag_name": tag, "prerelease": prerelease, "draft": false, "published_at": published,
               "html_url": format!("https://x.test/{tag}"),
               "assets": assets.iter().map(|a| asset(a)).collect::<Vec<_>>()})
    }
    const WIN: [&str; 2] = ["spartanport-windows.zip", "release-windows.json"];

    fn id(channel: Channel, version: &str, build: Option<&str>) -> ReleaseId {
        ReleaseId {
            channel,
            version: version.into(),
            build: build.map(String::from),
        }
    }

    fn rel(tag: &str, prerelease: bool, published: &str) -> Release {
        Release {
            id: ReleaseId::parse(tag, prerelease).unwrap(),
            published: published.into(),
            url: String::new(),
            page: String::new(),
        }
    }

    #[test]
    fn a_stable_tag_has_no_build() {
        assert_eq!(
            ReleaseId::parse("windows-v2.7.0", false),
            Some(id(Channel::Release, "2.7.0", None))
        );
    }

    #[test]
    fn a_test_build_tag_keeps_its_build() {
        assert_eq!(
            ReleaseId::parse("windows-v2.6.0-issue209-test1", true),
            Some(id(Channel::PreRelease, "2.6.0", Some("issue209-test1")))
        );
    }

    #[test]
    fn a_linux_tag_and_an_old_tag_parse() {
        assert_eq!(
            ReleaseId::parse("linux-v2.2.1-test1", true),
            Some(id(Channel::PreRelease, "2.2.1", Some("test1")))
        );
        assert_eq!(
            ReleaseId::parse("windows-v2.1.7-teste3", true),
            Some(id(Channel::PreRelease, "2.1.7", Some("teste3")))
        );
        assert_eq!(
            ReleaseId::parse("windows-v2.1", false),
            Some(id(Channel::Release, "2.1", None))
        );
    }

    #[test]
    fn a_stable_under_a_build_tag_follows_the_api() {
        let parsed = ReleaseId::parse("windows-v2.6.0-hotfix1", false).unwrap();
        assert_eq!(parsed.channel, Channel::Release);
        assert_eq!(parsed.build.as_deref(), Some("hotfix1"));
        assert_eq!(parsed.label("en"), "Release · 2.6.0 · hotfix1");
    }

    #[test]
    fn bad_tags_are_none() {
        for tag in [
            "v2.7.0",
            "windows-2.7.0",
            "windows-v",
            "windows-v2.6.0rc",
            "",
        ] {
            assert_eq!(ReleaseId::parse(tag, false), None, "{tag}");
        }
    }

    #[test]
    fn labels_name_the_channel() {
        let stable = id(Channel::Release, "2.6.0", None);
        let test = id(Channel::PreRelease, "2.6.0", Some("issue209-test1"));
        assert_eq!(stable.label("en"), "Release · 2.6.0");
        assert_eq!(test.label("en"), "Pre-release · 2.6.0 · issue209-test1");
        assert!(test.label("pt").starts_with("Pré-lançamento · "));
        assert_eq!(stable.short(), "2.6.0");
        assert_eq!(test.short(), "2.6.0 · issue209-test1");
    }

    #[test]
    fn same_build_ignores_the_channel() {
        let parse = |tag: &str, prerelease: bool| ReleaseId::parse(tag, prerelease).unwrap();
        assert!(parse("windows-v2.7.0", true).same_build(&parse("windows-v2.7.0", false)));
        assert!(!parse("windows-v2.7.0", false).same_build(&parse("windows-v2.7.1", false)));
        let test = parse("windows-v2.6.0-issue209-test1", true);
        assert!(!test.same_build(&parse("windows-v2.6.0", true)));
        assert!(test.same_build(&parse("windows-v2.6.0-issue209-test1", true)));
    }

    #[test]
    fn the_fallback_is_this_version() {
        assert_eq!(
            ReleaseId::fallback(),
            id(Channel::Release, crate::VERSION, None)
        );
    }

    fn mixed(extra_linux_asset: bool) -> Value {
        let mut stable = vec![
            "spartanport-windows.zip",
            "spartanport-x86_64.AppImage",
            "release-windows.json",
            "release-linux.json",
        ];
        if extra_linux_asset {
            stable.push("spartanport-x86_64.AppImage.sha256");
        }
        let python_era = entry(
            "windows-v2.6.0",
            false,
            "2026-01-01T00:00:00Z",
            &["spartanport-windows.zip"],
        );
        let mut draft = entry("windows-v2.5.0-test1", true, "2026-02-01T00:00:00Z", &WIN);
        draft["draft"] = json!(true);
        let mut no_date = entry("windows-v2.4.0", false, "x", &WIN);
        no_date.as_object_mut().unwrap().remove("published_at");
        json!([
            entry("windows-v2.7.0", false, "2026-05-01T00:00:00Z", &stable),
            entry(
                "windows-v2.6.0-issue209-test1",
                true,
                "2026-04-01T00:00:00Z",
                &WIN
            ),
            draft,
            python_era,
            entry(
                "linux-v2.7.1-test1",
                true,
                "2026-03-01T00:00:00Z",
                &["spartanport-x86_64.AppImage", "release-linux.json"]
            ),
            entry("nonsense", false, "2026-03-02T00:00:00Z", &WIN),
            no_date,
        ])
    }

    /// `mixed` plus the newest release, whose only stamp is the other system's.
    fn with_the_other_stamp(mut body: Value, package: &str, stamp: &str) -> Value {
        let newest = entry(
            "windows-v2.8.0",
            false,
            "2026-06-01T00:00:00Z",
            &[package, stamp],
        );
        body.as_array_mut().unwrap().push(newest);
        body
    }

    #[test]
    fn windows_takes_the_zip() {
        // The zip next to the Linux stamp only is not a stamped Windows package.
        let body = with_the_other_stamp(
            mixed(false),
            "spartanport-windows.zip",
            "release-linux.json",
        );
        let list = parse_releases(&body, true).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id.version, "2.7.0");
        assert_eq!(list[0].url, "https://x.test/spartanport-windows.zip");
        assert_eq!(list[0].page, "https://x.test/windows-v2.7.0");
        assert_eq!(
            list[1].id,
            id(Channel::PreRelease, "2.6.0", Some("issue209-test1"))
        );
    }

    #[test]
    fn linux_takes_the_appimage_not_its_checksum() {
        // The AppImage next to the Windows stamp only is not a stamped Linux package.
        let body = with_the_other_stamp(
            mixed(true),
            "spartanport-x86_64.AppImage",
            "release-windows.json",
        );
        let list = parse_releases(&body, false).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id.version, "2.7.0");
        assert_eq!(list[0].url, "https://x.test/spartanport-x86_64.AppImage");
        assert_eq!(list[1].id, id(Channel::PreRelease, "2.7.1", Some("test1")));
    }

    #[test]
    fn linux_has_no_package_while_the_release_has_no_appimage() {
        let body = json!([entry(
            "windows-v2.7.0",
            false,
            "2026-05-01T00:00:00Z",
            &[
                "spartanport-windows.zip",
                "release-windows.json",
                "release-linux.json"
            ]
        )]);
        assert!(parse_releases(&body, false).unwrap().is_empty());
        assert_eq!(parse_releases(&body, true).unwrap().len(), 1);
    }

    #[test]
    fn the_list_is_newest_first() {
        let body = json!([
            entry("windows-v2.6.0", false, "2026-01-01T00:00:00Z", &WIN),
            entry("windows-v2.8.0", false, "2026-03-01T00:00:00Z", &WIN),
            entry("windows-v2.7.0", false, "2026-02-01T00:00:00Z", &WIN),
        ]);
        let versions: Vec<_> = parse_releases(&body, true)
            .unwrap()
            .into_iter()
            .map(|r| r.id.version)
            .collect();
        assert_eq!(versions, ["2.8.0", "2.7.0", "2.6.0"]);
    }

    #[test]
    fn a_non_array_body_is_an_error() {
        let err = parse_releases(&json!({"message": "API rate limit exceeded"}), true).unwrap_err();
        assert!(err.to_string().contains("array"));
    }

    #[test]
    fn stables_stop_at_three_and_prereleases_do_not() {
        let mut list = Vec::new();
        for n in 0..5 {
            list.push(rel(&format!("windows-v2.{n}.0"), false, "d"));
        }
        for n in 0..10 {
            list.push(rel(&format!("windows-v3.0.0-t{n}"), true, "d"));
        }
        let stable: Vec<_> = stables(&list)
            .iter()
            .map(|r| r.id.version.clone())
            .collect();
        assert_eq!(stable, ["2.0.0", "2.1.0", "2.2.0"]);
        let pre = prereleases(&list);
        assert_eq!(pre.len(), 10);
        assert_eq!(pre[0].id.build.as_deref(), Some("t0"));
        assert_eq!(pre[9].id.build.as_deref(), Some("t9"));
    }

    #[test]
    fn the_newer_stable_beats_the_installed_version() {
        let list = vec![
            rel("windows-v2.8.0", false, "d"),
            rel("windows-v2.7.0", false, "d"),
            rel("windows-v2.6.0", false, "d"),
            rel("windows-v2.6.0-x", true, "d"),
        ];
        let stable = id(Channel::Release, "2.6.0", None);
        assert_eq!(newer_stable(&list, &stable).unwrap().id.version, "2.8.0");
        assert!(newer_stable(&list, &id(Channel::Release, "2.8.0", None)).is_none());
        let test = id(Channel::PreRelease, "2.6.0", Some("issue209-test1"));
        assert_eq!(newer_stable(&list, &test).unwrap().id.version, "2.8.0");
        assert!(newer_stable(&list[3..], &stable).is_none());
        assert!(newer_stable(&[], &stable).is_none());
    }
}
