use crate::components::creator_collections::by_creator;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_icons::{Icon, IconId};

use crate::app::invoke;
use crate::collected_links::{by_destination, CollectedLink, LinkSelection};
use crate::components::service_icon::{service_icon, service_name};
use crate::dom::hash_fragment;

#[derive(Serialize)]
struct DeleteLinksArgs {
    selection: LinkSelection,
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
    let delete_error = use_state(|| false);
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

    let delete = Callback::from({
        let delete_error = delete_error.clone();
        move |selection: LinkSelection| {
            let delete_error = delete_error.clone();
            delete_error.set(false);
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&DeleteLinksArgs {selection}).unwrap();
                if invoke("delete_collected_links", args).await.is_err() {
                    delete_error.set(true);
                }
            });
        }
    });

    let content = match rows.as_ref() {
        None => html! { <p id="links-loading" role="status">{"Loading links…"}</p> },
        Some(Err(err)) => html! { <p id="links-load-error" role="alert">{err}</p> },
        Some(Ok(links)) if links.is_empty() => html! { <p id="links-empty">{"No collected links yet."}</p> },
        Some(Ok(links)) => groups(links, &open_link, &delete, &expanded),
    };
    html! {
        <main id="links-page" class="container downloads link-catalog">
            <h1 id="links-heading">{"Links"}</h1>
            <div id="links-summary" class="summary">
                if *open_error { <p id="links-open-error" role="alert">{"Could not open this link in your default browser."}</p> }
                if *delete_error { <p id="links-delete-error" role="alert">{"Could not delete the selected links. Please try again."}</p> }
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

fn groups(links: &[CollectedLink], open: &Callback<String>, delete: &Callback<LinkSelection>, expanded: &UseStateHandle<BTreeSet<String>>) -> Html {
    html! { for by_destination(links).into_iter().map(|(service, collections)| {
        let group = format!("links-platform-{service}");
        let name = service_name(service);
        let count: usize = collections.values().map(Vec::len).sum();
        let totals = format!("{} · {}", quantity(collections.len(), "collection"), quantity(count, "link"));
        let all = LinkSelection::Destination {service: service.into()};
        html! {
            <section id={group.clone()} key={group.clone()} class="platform-block">
                <div id={format!("{group}-header")} class="platform-item">
                    <button type="button" id={format!("{group}-toggle")} class="link-group-toggle" aria-expanded={expanded.contains(&group).to_string()} aria-controls={format!("{group}-collections")} onclick={toggle_group(expanded, &group)}>
                        {service_icon(format!("{group}-icon"), service)}
                        <span id={format!("{group}-name")} class="item-title">{name}</span>
                        <span id={format!("{group}-count")} class="item-right">{totals}</span>
                    </button>
                    {trash_button(&group, format!("Delete all {name} links"), all, delete)}
                </div>
                <div id={format!("{group}-collections")} hidden={!expanded.contains(&group)}>
                    if expanded.contains(&group) {
                        {by_creator(&group, collections.into_iter().map(|((collection, origin, platform), mut entries)| {
                            let creator = entries.first().map(|row| row.creator.clone()).unwrap_or_default();
                            entries.sort_by_key(|link| &link.url);
                            let key = format!("{group}-collection-{}", hash_fragment(&format!("{platform}\0{collection}\0{origin}")));
                            let selection = LinkSelection::Collection {
                                service: service.into(), platform: platform.into(), collection: collection.into(), origin: origin.into(),
                            };
                            let source = service_name(platform);
                            (creator, html! {
                                <section key={key.clone()} id={key.clone()} class="collection-block">
                                    <div id={format!("{key}-header")} class="collection-item">
                                        <button type="button" id={format!("{key}-toggle")} class="link-group-toggle" aria-expanded={expanded.contains(&key).to_string()} aria-controls={format!("{key}-items")} onclick={toggle_group(expanded, &key)}>
                                            <span id={format!("{key}-name")} class="item-title">{format!("{collection} | {origin} · {source}")}</span>
                                            <span id={format!("{key}-count")} class="item-right">{quantity(entries.len(), "link")}</span>
                                        </button>
                                        {trash_button(&key, format!("Delete {name} links from {source} collection {collection}"), selection, delete)}
                                    </div>
                                    <div id={format!("{key}-items")} hidden={!expanded.contains(&key)}>
                                        if expanded.contains(&key) {
                                            <ul id={format!("{key}-list")} class="rows rows-card">
                                                {for entries.into_iter().map(|link| link_row(link, &key, open, delete))}
                                            </ul>
                                        }
                                    </div>
                                </section>
                            })
                        }))}
                    }
                </div>
            </section>
        }
    })}
}

fn trash_button(group: &str, label: String, selection: LinkSelection, delete: &Callback<LinkSelection>) -> Html {
    let onclick = delete.reform(move |_| selection.clone());
    html! {
        <button id={format!("{group}-delete")} type="button" class="icon-btn link-trash" aria-label={label.clone()} title={label} {onclick}>
            <Icon icon_id={IconId::LucideTrash2} width="18" height="18" />
        </button>
    }
}

fn link_row(link: &CollectedLink, collection: &str, open_link: &Callback<String>, delete: &Callback<LinkSelection>) -> Html {
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
            {trash_button(&row, format!("Delete {} link", service_name(&link.service)), LinkSelection::Link {link: link.clone()}, delete)}
        </li>
    }
}

fn quantity(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
