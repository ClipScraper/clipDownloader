use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::app::invoke;
use crate::components::service_icon::{service_icon, service_name};
use crate::dom::hash_fragment;

#[derive(Clone, PartialEq, Deserialize)]
struct CollectedLink {
    platform: String,
    collection: String,
    origin: String,
    url: String,
    service: String,
}

#[derive(Serialize)]
struct OpenLinkArgs {
    url: String,
}

#[derive(Properties, PartialEq)]
pub struct LinksProps {
    pub revision: u64,
}

#[function_component(LinksPage)]
pub fn links_page(props: &LinksProps) -> Html {
    let rows = use_state(|| None::<Result<Vec<CollectedLink>, String>>);
    let open_error = use_state(|| false);
    let expanded = use_state(BTreeSet::new);
    {
        let rows = rows.clone();
        use_effect_with(props.revision, move |_| {
            let current = Rc::new(Cell::new(true));
            let active = current.clone();
            spawn_local(async move {
                let result = match invoke("list_collected_links", JsValue::NULL).await {
                    Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
                    Err(err) => Err(err.as_string().unwrap_or_else(|| "Could not load collected links".into())),
                };
                if active.get() {
                    rows.set(Some(result));
                }
            });
            move || current.set(false)
        });
    }

    let open_link = Callback::from({
        let open_error = open_error.clone();
        move |url: String| {
            let open_error = open_error.clone();
            open_error.set(false);
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&OpenLinkArgs {url}).unwrap();
                if invoke("open_collected_link", args).await.is_err() {
                    open_error.set(true);
                }
            });
        }
    });

    let content = match rows.as_ref() {
        None => html! { <p id="links-loading" role="status">{"Loading links…"}</p> },
        Some(Err(err)) => html! { <p id="links-load-error" role="alert">{err}</p> },
        Some(Ok(links)) if links.is_empty() => html! { <p id="links-empty">{"No collected links yet."}</p> },
        Some(Ok(links)) => groups(links, &open_link, &expanded),
    };
    html! {
        <main id="links-page" class="container downloads link-catalog">
            <h1 id="links-heading">{"Links"}</h1>
            <div id="links-summary" class="summary">
                if *open_error { <p id="links-open-error" role="alert">{"Could not open this link in your default browser."}</p> }
                {content}
            </div>
        </main>
    }
}

fn toggle_group(expanded: &UseStateHandle<BTreeSet<String>>, key: &str) -> Callback<MouseEvent> {
    let expanded = expanded.clone();
    let key = key.to_owned();
    Callback::from(move |event: MouseEvent| {
        event.prevent_default();
        let mut next = (*expanded).clone();
        if !next.remove(&key) {
            next.insert(key.clone());
        }
        expanded.set(next);
    })
}

fn groups(links: &[CollectedLink], open_link: &Callback<String>, expanded: &UseStateHandle<BTreeSet<String>>) -> Html {
    let mut platforms = BTreeMap::<&str, BTreeMap<(&str, &str), BTreeMap<&str, &CollectedLink>>>::new();
    for link in links {
        let handle = if link.collection.is_empty() { "Unknown" } else { &link.collection };
        platforms.entry(&link.platform).or_default().entry((handle, &link.origin)).or_default().insert(&link.url, link);
    }
    html! { for platforms.into_iter().map(|(platform, collections)| {
        let group = format!("links-platform-{platform}");
        let count: usize = collections.values().map(BTreeMap::len).sum();
        html! {
            <details id={group.clone()} key={group.clone()} open={expanded.contains(&group)} class="platform-block">
                <summary id={format!("{group}-toggle")} class="platform-item" onclick={toggle_group(expanded, &group)}>
                    <span id={format!("{group}-identity")} class="item-left">
                        {service_icon(format!("{group}-icon"), platform)}
                        <span id={format!("{group}-name")} class="item-title">{service_name(platform)}</span>
                    </span>
                    <span id={format!("{group}-count")} class="item-right">{format!("{} · {}", quantity(collections.len(), "collection"), quantity(count, "link"))}</span>
                </summary>
                if expanded.contains(&group) {
                    {for collections.into_iter().map(|((name, origin), entries)| {
                        let collection = format!("{group}-collection-{}", hash_fragment(&format!("{name}\0{origin}")));
                        html! {
                            <details key={collection.clone()} id={collection.clone()} class="collection-block" open={expanded.contains(&collection)}>
                                <summary id={format!("{collection}-toggle")} class="collection-item" onclick={toggle_group(expanded, &collection)}>
                                    <span id={format!("{collection}-name")} class="item-title">{format!("{name} | {origin}")}</span>
                                    <span id={format!("{collection}-count")} class="item-right">{quantity(entries.len(), "link")}</span>
                                </summary>
                                if expanded.contains(&collection) {
                                    <ul id={format!("{collection}-list")} class="rows rows-card">
                                        {for entries.into_values().map(|link| link_row(link, &collection, open_link))}
                                    </ul>
                                }
                            </details>
                        }
                    })}
                }
            </details>
        }
    })}
}

fn link_row(link: &CollectedLink, collection: &str, open_link: &Callback<String>) -> Html {
    let row = format!("{collection}-link-{}", hash_fragment(&link.url));
    let url = link.url.clone();
    let open = open_link.reform(move |event: MouseEvent| {
        event.prevent_default();
        url.clone()
    });
    let middle = {
        let open = open.clone();
        Callback::from(move |event: MouseEvent| {
            if event.button() == 1 {
                open.emit(event);
            }
        })
    };
    html! {
        <li key={row.clone()} id={row.clone()} class="row-line">
            <a id={format!("{row}-open")} class="collected-link" href={link.url.clone()} onclick={open} onauxclick={middle} title="Open in default browser">
                {service_icon(format!("{row}-icon"), &link.service)}
                <span id={format!("{row}-address")} class="link-address">{&link.url}</span>
            </a>
        </li>
    }
}

fn quantity(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
