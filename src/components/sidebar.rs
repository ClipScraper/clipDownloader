use yew::prelude::*;
use yew_icons::{Icon, IconId};
use crate::app::Page;

#[derive(Properties, PartialEq)]
pub struct SidebarProps {
    pub page: UseStateHandle<Page>,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let set_page = |p: Page, page_handle: UseStateHandle<Page>| Callback::from(move |_| page_handle.set(p));

    html! {
        <aside class="sidebar">
            <button class="nav-btn" onclick={set_page(Page::Home, props.page.clone())} title="Home"><Icon icon_id={IconId::LucideHome} width={"28"} height={"28"} /></button>
            <button class="nav-btn" onclick={set_page(Page::Downloads, props.page.clone())} title="Downloads"><Icon icon_id={IconId::LucideDownload} width={"28"} height={"28"} /></button>
            <button class="nav-btn" onclick={set_page(Page::Library, props.page.clone())} title="Library"><Icon icon_id={IconId::LucideLibrary} width={"28"} height={"28"} /></button>
            <button class="nav-btn" onclick={set_page(Page::Settings, props.page.clone())} title="Settings"><Icon icon_id={IconId::LucideSettings} width={"28"} height={"28"} /></button>
            <button class="nav-btn" onclick={set_page(Page::Extension, props.page.clone())} title="Extension"><Icon icon_id={IconId::LucideFolder} width={"28"} height={"28"} /></button>
            <button class="nav-btn" onclick={set_page(Page::Sponsor, props.page.clone())} title="Sponsor"><Icon icon_id={IconId::LucideHeart} width={"28"} height={"28"} /></button>
        </aside>
    }
}
