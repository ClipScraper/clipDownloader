use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CollectedLink {
    #[serde(default)]
    pub creator: String,
    pub platform: String,
    pub collection: String,
    pub origin: String,
    pub url: String,
    pub service: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "level", rename_all = "lowercase")]
pub enum LinkSelection {
    Destination { service: String },
    Collection { service: String, platform: String, collection: String, origin: String },
    Link { link: CollectedLink },
}

#[cfg(any(target_arch = "wasm32", test))]
pub fn by_destination(links: &[CollectedLink]) -> std::collections::BTreeMap<&str, SourceCollections<'_>> {
    let mut groups = std::collections::BTreeMap::<&str, SourceCollections<'_>>::new();
    for link in links {
        groups.entry(&link.service).or_default()
            .entry((&link.collection, &link.origin, &link.platform)).or_default().push(link);
    }
    groups
}

#[cfg(any(target_arch = "wasm32", test))]
type SourceCollections<'a> = std::collections::BTreeMap<(&'a str, &'a str, &'a str), Vec<&'a CollectedLink>>;
