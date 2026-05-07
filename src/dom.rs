use std::collections::HashSet;

use web_sys::{window, Element};

pub fn slug_fragment(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !slug.is_empty() && !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "item".into()
    } else {
        trimmed.to_string()
    }
}

pub fn hash_fragment(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:x}")
}

pub fn assign_missing_descriptive_ids(root_id: &str) {
    let Some(document) = window().and_then(|window| window.document()) else {
        return;
    };
    let Some(root) = document.get_element_by_id(root_id) else {
        return;
    };

    let mut used_ids = HashSet::new();
    collect_existing_ids(&root, &mut used_ids);
    assign_child_ids(&root, root_id, "root", &mut used_ids);
}

fn collect_existing_ids(element: &Element, used_ids: &mut HashSet<String>) {
    if !element.id().is_empty() {
        used_ids.insert(element.id());
    }

    let children = element.children();
    for index in 0..children.length() {
        if let Some(child) = children.item(index) {
            collect_existing_ids(&child, used_ids);
        }
    }
}

fn assign_child_ids(element: &Element, root_id: &str, path: &str, used_ids: &mut HashSet<String>) {
    let children = element.children();
    for index in 0..children.length() {
        if let Some(child) = children.item(index) {
            let child_path = format!("{path}-{}", index + 1);
            if child.id().is_empty() {
                let descriptor = describe_element(&child);
                let base_id = format!(
                    "{root_id}-{}-{}",
                    slug_fragment(&descriptor),
                    hash_fragment(&format!("{root_id}|{child_path}|{descriptor}"))
                );
                let id = ensure_unique_id(base_id, used_ids);
                child.set_id(&id);
                used_ids.insert(id);
            } else {
                used_ids.insert(child.id());
            }

            assign_child_ids(&child, root_id, &child_path, used_ids);
        }
    }
}

fn ensure_unique_id(mut candidate: String, used_ids: &HashSet<String>) -> String {
    if !used_ids.contains(&candidate) {
        return candidate;
    }

    let base = candidate.clone();
    let mut index = 2usize;
    while used_ids.contains(&candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    candidate
}

fn describe_element(element: &Element) -> String {
    let mut parts = vec![element.tag_name().to_ascii_lowercase()];

    if let Some(value) = first_non_empty_attribute(
        element,
        &[
            "title",
            "aria-label",
            "placeholder",
            "name",
            "type",
            "for",
            "href",
            "src",
        ],
    ) {
        parts.push(value);
    }

    if let Some(text) = meaningful_text(element) {
        parts.push(text);
    } else if let Some(class_name) = first_class_name(element) {
        parts.push(class_name);
    }

    parts
        .into_iter()
        .map(|part| truncate_chars(&part, 48))
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_non_empty_attribute(element: &Element, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| element.get_attribute(name))
        .map(|value| simplify_value(&value))
        .filter(|value| !value.is_empty())
}

fn first_class_name(element: &Element) -> Option<String> {
    element
        .get_attribute("class")
        .and_then(|value| value.split_whitespace().next().map(ToString::to_string))
        .map(|value| simplify_value(&value))
        .filter(|value| !value.is_empty())
}

fn meaningful_text(element: &Element) -> Option<String> {
    element
        .text_content()
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|value| simplify_value(&value))
        .filter(|value| !value.is_empty())
}

fn simplify_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.contains('/') || trimmed.contains('#') || trimmed.contains('?') {
        let parts: Vec<&str> = trimmed
            .split(&['/', '#', '?'][..])
            .filter(|part| !part.trim().is_empty())
            .collect();
        if parts.is_empty() {
            trimmed.to_string()
        } else {
            parts
                .iter()
                .rev()
                .take(2)
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join(" ")
        }
    } else {
        trimmed.to_string()
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    while truncated.ends_with('-') || truncated.ends_with(' ') {
        truncated.pop();
    }
    truncated
}
