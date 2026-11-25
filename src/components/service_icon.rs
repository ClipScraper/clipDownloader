use yew::prelude::*;
use yew_icons::{Icon, IconId};

pub fn service_name(service: &str) -> &str {
    match service {
        "instagram" => "Instagram",
        "twitter" => "X / Twitter",
        "mega" => "MEGA",
        "dropbox" => "Dropbox",
        "telegram" => "Telegram",
        "tiktok" => "TikTok",
        "youtube" => "YouTube",
        "pinterest" => "Pinterest",
        "patreon" => "Patreon",
        "other" => "Other",
        name => name,
    }
}

pub fn service_icon(id: String, service: &str) -> Html {
    let name = if service == "other" { "Link" } else { service_name(service) }.to_owned();
    let asset = match service {
        "instagram" => Some("public/instagram.webp"),
        "twitter" => Some("public/x.com.png"),
        "tiktok" => Some("public/tiktok.webp"),
        "youtube" => Some("public/youtube.webp"),
        "pinterest" => Some("public/pinterest.png"),
        "patreon" => Some("public/patreon.png"),
        _ => None,
    };
    if let Some(src) = asset {
        return html! { <img {id} class="brand-icon" {src} alt={name.clone()} title={name} /> };
    }
    let (icon, class) = match service {
        "mega" => (IconId::SimpleIconsMega, "service-mega"),
        "dropbox" => (IconId::SimpleIconsDropbox, "service-dropbox"),
        "telegram" => (IconId::SimpleIconsTelegram, "service-telegram"),
        _ => (IconId::LucideLink, "service-generic"),
    };
    html! {
        <span {id} class={classes!("brand-icon", "service-icon", class)} role="img" aria-label={name.clone()} title={name}>
            <Icon icon_id={icon} width="18" height="18" />
        </span>
    }
}
