use std::collections::BTreeMap;

use yew::prelude::*;
use yew_icons::{Icon, IconId};

use crate::dom::hash_fragment;
use crate::types::{content_type_str, ClipRow};

pub fn collection_title(row: &ClipRow) -> String {
    let handle = if row.handle.trim().is_empty() { "Unknown" } else { &row.handle };
    let typ = content_type_str(&row.content_type);
    if row.creator.is_empty() {
        format!("{handle} | {typ}")
    } else {
        format!("{} › {handle} | {typ}", row.creator)
    }
}

/// Collections with known creators get a profile level. Older imports keep their existing layout.
pub fn by_creator(parent: &str, collections: impl Iterator<Item = (String, Html)>) -> Html {
    let mut profiles = BTreeMap::<String, Vec<Html>>::new();
    for (creator, collection) in collections {
        profiles.entry(creator).or_default().push(collection);
    }
    html! { for profiles.into_iter().map(|(creator, collections)| {
        if creator.is_empty() {
            return html! { for collections };
        }
        let id = format!("{parent}-creator-{}", hash_fragment(&creator));
        let count = collections.len();
        html! {
            <details id={id.clone()} key={id.clone()} class="creator-collections">
                <summary id={format!("{id}-toggle")} class="creator-item">
                    <Icon icon_id={IconId::LucideUser} width="18" height="18" />
                    <span id={format!("{id}-name")} class="item-title">{creator}</span>
                    <span id={format!("{id}-count")} class="item-right">{format!("{count} collection{}", if count == 1 { "" } else { "s" })}</span>
                </summary>
                <div id={format!("{id}-collections")}>{for collections}</div>
            </details>
        }
    })}
}
