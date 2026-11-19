use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use js_sys::Uint8Array;

mod inventory;
mod auth;
mod cloud;

#[derive(Clone, Serialize, Deserialize)]
struct Sheet {
    name: String,
    race: String,
    specialization: String,
    community: String,
    class_name: String,
    level: u8,
    pv: i32,
    san: i32,
    pm: i32,
    #[serde(default)]
    pv_max: i32,
    #[serde(default)]
    san_max: i32,
    #[serde(default)]
    pm_max: i32,
    stats: [i32; 5],
    #[serde(default)]
    test_advantages: Vec<i32>,
    abilities: String,
    capacity: u8,
    notes: String,
    #[serde(default)]
    portrait_path: Option<String>,
    #[serde(default = "inventory::default_inventory_width")]
    pub(crate) inventory_width: u8,
    #[serde(default = "inventory::default_inventory_height")]
    pub(crate) inventory_height: u8,
    #[serde(default)]
    pub(crate) inventory_items: Vec<inventory::InventoryItem>,
}

impl Default for Sheet {
    fn default() -> Self {
        Self {
            name: "Novo personagem".into(),
            race: String::new(),
            specialization: String::new(),
            community: String::new(),
            class_name: String::new(),
            level: 1,
            pv: 0,
            san: 0,
            pm: 0,
            pv_max: 0,
            san_max: 0,
            pm_max: 0,
            stats: [0; 5],
            test_advantages: vec![0; 16],
            abilities: String::new(),
            capacity: 3,
            notes: String::new(),
            portrait_path: None,
            inventory_width: 6,
            inventory_height: 6,
            inventory_items: Vec::new(),
        }
    }
}


fn handle_portrait_change(
    ev: web_sys::Event,
    session: auth::AuthSession,
    character_id: String,
    sheet: RwSignal<Sheet>,
    save_status: RwSignal<String>,
    portrait_url: RwSignal<Option<String>>,
    portrait_uploading: RwSignal<bool>,
) {
    let input = ev.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok());
    let file = input.and_then(|input| input.files().and_then(|files| files.get(0)));
    let Some(file) = file else { return; };

    let mime = file.type_();
    let extension = match mime.as_str() {
        "image/jpeg" => String::from("jpg"),
        "image/png" => String::from("png"),
        "image/webp" => String::from("webp"),
        _ => {
            save_status.set("Formato inválido. Use JPG, PNG ou WEBP.".to_string());
            return;
        }
    };

    if file.size() > 5.0 * 1024.0 * 1024.0 {
        save_status.set("A imagem deve ter no máximo 5 MB.".to_string());
        return;
    }

    portrait_uploading.set(true);
    save_status.set("Enviando imagem...".to_string());

    spawn_local(async move {
        let result = async {
            let buffer = JsFuture::from(file.array_buffer()).await
                .map_err(|_| "Não foi possível ler a imagem.".to_string())?;
            let bytes = Uint8Array::new(&buffer).to_vec();
            cloud::upload_portrait(
                &session,
                &character_id,
                &bytes,
                &mime,
                &extension,
            ).await
        }.await;

        match result {
            Ok((path, url)) => {
                sheet.update(|s| s.portrait_path = Some(path));
                portrait_url.set(Some(url));
                save_status.set("Imagem enviada. Clique em Salvar ficha para confirmar.".to_string());
            }
            Err(error) => save_status.set(format!("Erro ao enviar imagem: {}", error)),
        }

        portrait_uploading.set(false);
    });
}

async fn prepare_account(
    session: auth::AuthSession,
) -> Result<(auth::AuthSession, Sheet, String), String> {
    let character = match cloud::load_character(&session).await? {
        Some(character) => character,
        None => {
            let default_sheet = Sheet::default();
            let data = serde_json::to_value(&default_sheet)
                .map_err(|e| e.to_string())?;

            cloud::create_character(
                &session,
                &default_sheet.name,
                data,
            )
            .await?
        }
    };

    let stored_portrait_path = character.portrait_path.clone();
    let mut sheet = serde_json::from_value::<Sheet>(character.sheet_data)
        .unwrap_or_default();
    if sheet.portrait_path.is_none() {
        sheet.portrait_path = stored_portrait_path;
    }

    Ok((session, sheet, character.id))
}

fn load_sheet() -> Sheet {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("etheris-sheet-v1").ok().flatten())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn save_sheet(sheet: &Sheet) {
    if let (Some(storage), Ok(json)) = (
        web_sys::window().and_then(|w| w.local_storage().ok().flatten()),
        serde_json::to_string(sheet),
    ) {
        let _ = storage.set_item("etheris-sheet-v1", &json);
    }
}

fn load_color_theme(character_id: &str) -> String {
    let storage_key = format!("samsara-color-theme-{}", character_id);
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(&storage_key).ok().flatten())
        .filter(|theme| matches!(theme.as_str(), "green" | "blue" | "red" | "yellow" | "purple" | "mono"))
        .unwrap_or_else(|| "green".to_string())
}

fn specialization_options_for_race(race: &str) -> Vec<&'static str> {
    match race {
        "Humano" => vec!["Amalgamos", "Flagelos", "Aplicadores", "Suplícios"],
        "Áureo" => vec!["Místico", "Corpóreo"],
        "Pesadelo" => vec!["Rei", "Dama", "Valete"],
        _ => vec![],
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct SkillEntry {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    children: Vec<SkillEntry>,
}

fn load_skill_items(raw: &str) -> Vec<SkillEntry> {
    if let Ok(items) = serde_json::from_str::<Vec<SkillEntry>>(raw) {
        return items;
    }
    if let Ok(items) = serde_json::from_str::<Vec<(String, String)>>(raw) {
        return items.into_iter().map(|(name, description)| SkillEntry {
            name,
            description,
            children: Vec::new(),
        }).collect();
    }
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            if let Some((name, description)) = line.split_once("::") {
                let name = name.trim();
                if name.is_empty() { None } else {
                    Some(SkillEntry {
                        name: name.to_string(),
                        description: decode_skill_description(description.trim()),
                        children: Vec::new(),
                    })
                }
            } else {
                Some(SkillEntry {
                    name: line.to_string(),
                    description: String::new(),
                    children: Vec::new(),
                })
            }
        })
        .collect()
}

fn remove_skill_entry(items: &mut Vec<SkillEntry>, name: &str) -> bool {
    if let Some(index) = items.iter().position(|item| item.name == name) {
        items.remove(index);
        return true;
    }
    for item in items.iter_mut() {
        if let Some(index) = item.children.iter().position(|child| child.name == name) {
            item.children.remove(index);
            return true;
        }
    }
    false
}

fn merge_skill_entries(items: &mut Vec<SkillEntry>, source_name: &str, target_name: &str) -> bool {
    if source_name == target_name { return false; }
    let Some(source_index) = items.iter().position(|item| item.name == source_name) else {
        return false;
    };
    let Some(target_index) = items.iter().position(|item| item.name == target_name) else {
        return false;
    };
    if source_index == target_index { return false; }

    let source = items.remove(source_index);
    let adjusted_target = if source_index < target_index { target_index - 1 } else { target_index };
    let folder_number = items.len() + 1;
    let target = &mut items[adjusted_target];

    if target.children.is_empty() {
        let original_target = SkillEntry {
            name: target.name.clone(),
            description: target.description.clone(),
            children: Vec::new(),
        };
        target.name = format!("Pasta {}", folder_number);
        target.description.clear();
        target.children = vec![original_target, source];
    } else {
        target.children.push(source);
    }
    true
}

fn encode_skill_description(description: &str) -> String {
    description.replace('\n', "\\n")
}

fn decode_skill_description(description: &str) -> String {
    description.replace("\\n", "\n")
}

#[component]
fn SheetApp(
    initial_sheet: Sheet,
    session: auth::AuthSession,
    character_id: String,
    on_logout: Callback<()>,
) -> impl IntoView {
    let initial_portrait_path = initial_sheet.portrait_path.clone();
    let sheet = RwSignal::new(initial_sheet);
    // Handles Copy evitam que closures internas do view! movam dados não-Copy.
    let session = RwSignal::new(session);
    let character_id = RwSignal::new(character_id);
    let save_status = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let tab = RwSignal::new(0usize);
    let resource_editing = RwSignal::new(false);
    let skill_name = RwSignal::new(String::new());
    let skill_description = RwSignal::new(String::new());
    let skill_form_open = RwSignal::new(false);
    let expanded_skill_folders = RwSignal::new(Vec::<String>::new());
    let editing_skills = RwSignal::new(Vec::<String>::new());
    let portrait_url = RwSignal::new(None::<String>);
    let portrait_uploading = RwSignal::new(false);
    let selected_theme = RwSignal::new(load_color_theme(&character_id.get()));
    let theme_picker_open = RwSignal::new(false);
    let is_master = session.get_untracked().user.id == cloud::MASTER_UID;
    let viewing_as_master = RwSignal::new(false);
    let master_characters = RwSignal::new(Vec::<(cloud::CloudCharacter, Option<String>)>::new());
    let master_loading = RwSignal::new(false);
    let master_error = RwSignal::new(String::new());
    let master_delete_pending = RwSignal::new(String::new());

    Effect::new(move |_| {
        let storage_key = format!("samsara-color-theme-{}", character_id.get());
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item(&storage_key, &selected_theme.get());
        }
    });

    if let Some(path) = initial_portrait_path {
        let current_session = session.get();
        spawn_local(async move {
            match cloud::portrait_signed_url(&current_session, &path).await {
                Ok(url) => portrait_url.set(Some(url)),
                Err(error) => save_status.set(format!("Não foi possível carregar o retrato: {}", error)),
            }
        });
    }

    Effect::new(move |_| save_sheet(&sheet.get()));

    let skill_items = move || load_skill_items(&sheet.get().abilities);

    let save_skill_items = move |items: Vec<SkillEntry>| {
        sheet.update(|s| {
            s.abilities = serde_json::to_string(&items).unwrap_or_default();
        });
    };

    let add_skill = move |_| {
        let name = skill_name.get().trim().to_string();
        let description = skill_description.get().trim().to_string();

        if name.is_empty() {
            return;
        }

        let mut items = skill_items();
        if !items.iter().any(|item| item.name.eq_ignore_ascii_case(&name)) {
            items.push(SkillEntry {
                name,
                description,
                children: Vec::new(),
            });
        }
        save_skill_items(items);
        skill_name.set(String::new());
        skill_description.set(String::new());
    };

    let remove_skill = move |skill_to_remove: String| {
        let mut items = skill_items();
        remove_skill_entry(&mut items, &skill_to_remove);
        save_skill_items(items);
    };

    let drop_skill_on = move |target_name: String, ev: web_sys::DragEvent| {
        ev.prevent_default();
        let source_name = ev.data_transfer()
            .and_then(|data| data.get_data("text/plain").ok())
            .unwrap_or_default();
        if source_name.is_empty() || source_name == target_name {
            return;
        }

        let mut items = skill_items();
        if merge_skill_entries(&mut items, &source_name, &target_name) {
            let folder_name = items.iter()
                .find(|item| (item.name == target_name && item.children.iter().any(|child| child.name == source_name))
                    || (item.children.iter().any(|child| child.name == source_name)
                        && item.children.iter().any(|child| child.name == target_name)))
                .map(|item| item.name.clone());
            save_skill_items(items);
            if let Some(folder_name) = folder_name {
                expanded_skill_folders.update(|expanded| {
                    if !expanded.contains(&folder_name) {
                        expanded.push(folder_name);
                    }
                });
            }
        }
    };

    let submit_skill = move |_| {
        if !skill_form_open.get() {
            skill_form_open.set(true);
            return;
        }

        let has_name = !skill_name.get().trim().is_empty();
        if has_name {
            add_skill(());
            skill_form_open.set(false);
        }
    };

    let render_skill_list = move || {
        let items = skill_items();

        if items.is_empty() {
            view! {
                <p class="skill-empty">"Nenhuma habilidade adicionada."</p>
            }
            .into_any()
        } else {
            view! {
                <div class="item-list skill-item-list">
                    {items.into_iter().map(|entry| {
                        let entry_name = entry.name.clone();
                        let entry_name_for_drag = entry.name.clone();
                        let entry_name_for_drop = entry.name.clone();
                        let entry_name_for_remove = entry.name.clone();
                        let entry_name_for_edit = entry.name.clone();
                        let entry_name_for_save = entry.name.clone();
                        let entry_name_for_description = entry.name.clone();
                        let is_editing = editing_skills.get().contains(&entry.name);

                        if entry.children.is_empty() {
                            let description = if entry.description.is_empty() {
                                "Nenhuma descrição registrada.".to_string()
                            } else {
                                entry.description.clone()
                            };
                            view! {
                                <div
                                    class="item skill-entry"
                                    draggable="true"
                                    on:dragstart=move |ev: web_sys::DragEvent| {
                                        if let Some(data) = ev.data_transfer() {
                                            let _ = data.set_data("text/plain", &entry_name_for_drag);
                                            data.set_effect_allowed("move");
                                        }
                                    }
                                    on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                                    on:drop=move |ev: web_sys::DragEvent| drop_skill_on(entry_name_for_drop.clone(), ev)
                                >
                                    <div class="item-title skill-entry-heading">
                                        {if is_editing {
                                            view! {
                                                <input
                                                    class="skill-edit-name"
                                                    aria-label="Nome da habilidade"
                                                    prop:value=entry_name.clone()
                                                    on:change=move |ev| {
                                                        let new_name = event_target_value(&ev).trim().to_string();
                                                        if new_name.is_empty() { return; }
                                                        let mut current = skill_items();
                                                        if let Some(skill) = current.iter_mut().find(|item| item.name == entry_name_for_save) {
                                                            skill.name = new_name.clone();
                                                            save_skill_items(current);
                                                            editing_skills.update(|names| {
                                                                if let Some(n) = names.iter_mut().find(|n| *n == &entry_name_for_save) {
                                                                    *n = new_name;
                                                                }
                                                            });
                                                        }
                                                    }
                                                />
                                            }.into_any()
                                        } else {
                                            view! { <strong>{entry.name.clone()}</strong> }.into_any()
                                        }}
                                        <div class="skill-entry-actions">
                                            <button
                                                type="button"
                                                class="inventory-icon-button inventory-delete-icon skill-remove-icon"
                                                aria-label="Remover habilidade"
                                                title="Remover habilidade"
                                                on:click=move |_| remove_skill(entry_name_for_remove.clone())
                                            >
                                                <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg>
                                            </button>
                                            <button
                                                type="button"
                                                class="skill-edit-button"
                                                on:click=move |_| editing_skills.update(|names| {
                                                    if let Some(index) = names.iter().position(|name| name == &entry_name_for_edit) {
                                                        names.remove(index);
                                                    } else {
                                                        names.push(entry_name_for_edit.clone());
                                                    }
                                                })
                                            >{if is_editing { "Concluir" } else { "Editar" }}</button>
                                        </div>
                                    </div>
                                    {if is_editing {
                                        view! {
                                            <textarea
                                                class="skill-edit-description"
                                                rows="2"
                                                aria-label="Descrição da habilidade"
                                                prop:value=entry.description.clone()
                                                on:change=move |ev| {
                                                    let value = event_target_value(&ev);
                                                    let mut current = skill_items();
                                                    if let Some(skill) = current.iter_mut().find(|item| item.name == entry_name_for_description) {
                                                        skill.description = value;
                                                        save_skill_items(current);
                                                    }
                                                }
                                            ></textarea>
                                        }.into_any()
                                    } else {
                                        view! { <p>{description}</p> }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        } else {
                            let folder_name_for_toggle = entry.name.clone();
                            let folder_name_for_drop = entry.name.clone();
                            let folder_name_for_description = entry.name.clone();
                            let folder_name_for_rename = entry.name.clone();
                            let folder_name_for_remove = entry.name.clone();
                            let is_expanded = expanded_skill_folders.get().contains(&entry.name);
                            let folder_description = entry.description.clone();
                            let folder_name = entry.name.clone();
                            let children = entry.children.clone();
                            view! {
                                <div
                                    class="item skill-folder"
                                    on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                                    on:drop=move |ev: web_sys::DragEvent| drop_skill_on(folder_name_for_drop.clone(), ev)
                                >
                                    <div class="item-title skill-folder-heading">
                                        {if is_expanded {
                                            view! {
                                                <input
                                                    class="skill-folder-name"
                                                    aria-label="Nome da pasta"
                                                    prop:value=folder_name.clone()
                                                    on:change=move |ev| {
                                                        let new_name = event_target_value(&ev).trim().to_string();
                                                        if new_name.is_empty() { return; }
                                                        let mut current = skill_items();
                                                        if current.iter().any(|item| item.name == new_name && item.name != folder_name_for_rename) {
                                                            return;
                                                        }
                                                        if let Some(folder) = current.iter_mut().find(|item| item.name == folder_name_for_rename) {
                                                            folder.name = new_name.clone();
                                                            save_skill_items(current);
                                                            expanded_skill_folders.update(|expanded| {
                                                                for name in expanded.iter_mut() {
                                                                    if name == &folder_name_for_rename {
                                                                        *name = new_name.clone();
                                                                    }
                                                                }
                                                            });
                                                            editing_skills.update(|names| {
                                                                for name in names.iter_mut() {
                                                                    if name == &folder_name_for_rename {
                                                                        *name = new_name.clone();
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    }
                                                />
                                            }.into_any()
                                        } else {
                                            view! { <strong>{folder_name.clone()}</strong> }.into_any()
                                        }}
                                        <div class="skill-folder-actions">
                                            <button
                                                type="button"
                                                class="inventory-icon-button inventory-delete-icon skill-remove-icon"
                                                aria-label="Remover habilidade"
                                                title="Remover habilidade"
                                                on:click=move |_| remove_skill(folder_name_for_remove.clone())
                                            >
                                                <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg>
                                            </button>
                                            <button
                                                type="button"
                                                class="skill-folder-toggle"
                                                aria-label="Expandir ou recolher pasta"
                                                on:click=move |_| {
                                                    expanded_skill_folders.update(|expanded| {
                                                        if let Some(index) = expanded.iter().position(|name| name == &folder_name_for_toggle) {
                                                            expanded.remove(index);
                                                        } else {
                                                            expanded.push(folder_name_for_toggle.clone());
                                                        }
                                                    });
                                                    editing_skills.update(|names| names.retain(|name| name != &folder_name_for_toggle));
                                                }
                                            >
                                                {if is_expanded {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                            <path d="m6 9 6 6 6-6"/>
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                            <path d="m9 6 6 6-6 6"/>
                                                        </svg>
                                                    }.into_any()
                                                }}
                                            </button>
                                        </div>
                                    </div>
                                    {if is_expanded {
                                        view! {
                                            <div class="skill-folder-details">
                                                {
                                                    view! {
                                                        <textarea
                                                            class="skill-folder-description"
                                                            rows="2"
                                                            placeholder="Descrição da pasta..."
                                                            aria-label="Descrição da pasta"
                                                            prop:value=folder_description.clone()
                                                            on:change=move |ev| {
                                                                let value = event_target_value(&ev);
                                                                let mut current = skill_items();
                                                                if let Some(folder) = current.iter_mut().find(|item| item.name == folder_name_for_description) {
                                                                    folder.description = value;
                                                                    save_skill_items(current);
                                                                }
                                                            }
                                                        ></textarea>
                                                    }.into_any()
                                                }
                                                <div class="skill-folder-contents">
                                                    {children.into_iter().map(|child| {
                                                        let child_name = child.name.clone();
                                                        let child_name_for_remove = child.name.clone();
                                                        let child_name_for_edit = child.name.clone();
                                                        let child_name_for_save = child.name.clone();
                                                        let child_name_for_description = child.name.clone();
                                                        let child_description = if child.description.is_empty() {
                                                            "Nenhuma descrição registrada.".to_string()
                                                        } else {
                                                            child.description.clone()
                                                        };
                                                        let child_editing = editing_skills.get().contains(&child.name);
                                                        view! {
                                                            <div class="skill-folder-child">
                                                                <div class="item-title skill-entry-heading">
                                                                    {if child_editing {
                                                                        view! {
                                                                            <input
                                                                                class="skill-edit-name"
                                                                                aria-label="Nome da habilidade"
                                                                                prop:value=child_name.clone()
                                                                                on:change=move |ev| {
                                                                                    let new_name = event_target_value(&ev).trim().to_string();
                                                                                    if new_name.is_empty() { return; }
                                                                                    let mut current = skill_items();
                                                                                    if let Some(folder) = current.iter_mut().find(|item| item.children.iter().any(|child| child.name == child_name_for_save)) {
                                                                                        if let Some(skill) = folder.children.iter_mut().find(|child| child.name == child_name_for_save) {
                                                                                            skill.name = new_name.clone();
                                                                                            save_skill_items(current);
                                                                                            editing_skills.update(|names| {
                                                                                                if let Some(name) = names.iter_mut().find(|name| *name == &child_name_for_save) {
                                                                                                    *name = new_name;
                                                                                                }
                                                                                            });
                                                                                        }
                                                                                    }
                                                                                }
                                                                            />
                                                                        }.into_any()
                                                                    } else {
                                                                        view! { <strong>{child.name.clone()}</strong> }.into_any()
                                                                    }}
                                                                    <div class="skill-entry-actions">
                                                                        <button
                                                type="button"
                                                class="inventory-icon-button inventory-delete-icon skill-remove-icon"
                                                aria-label="Remover habilidade"
                                                title="Remover habilidade"
                                                on:click=move |_| remove_skill(child_name_for_remove.clone())
                                            >
                                                <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg>
                                            </button>
                                                                        <button
                                                                            type="button"
                                                                            class="skill-edit-button"
                                                                            on:click=move |_| editing_skills.update(|names| {
                                                                                if let Some(index) = names.iter().position(|name| name == &child_name_for_edit) {
                                                                                    names.remove(index);
                                                                                } else {
                                                                                    names.push(child_name_for_edit.clone());
                                                                                }
                                                                            })
                                                                        >{if child_editing { "Concluir" } else { "Editar" }}</button>
                                                                    </div>
                                                                </div>
                                                                {if child_editing {
                                                                    view! {
                                                                        <textarea
                                                                            class="skill-edit-description"
                                                                            rows="2"
                                                                            aria-label="Descrição da habilidade"
                                                                            prop:value=child.description.clone()
                                                                            on:change=move |ev| {
                                                                                let value = event_target_value(&ev);
                                                                                let mut current = skill_items();
                                                                                if let Some(folder) = current.iter_mut().find(|item| item.children.iter().any(|child| child.name == child_name_for_description)) {
                                                                                    if let Some(skill) = folder.children.iter_mut().find(|child| child.name == child_name_for_description) {
                                                                                        skill.description = value;
                                                                                        save_skill_items(current);
                                                                                    }
                                                                                }
                                                                            }
                                                                        ></textarea>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <p>{child_description}</p> }.into_any()
                                                                }}
                                                            </div>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <p class="skill-folder-summary">{if folder_description.is_empty() { "Nenhuma descrição registrada.".to_string() } else { folder_description.clone() }}</p> }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    }).collect_view()}
                </div>
            }
            .into_any()
        }
    };

    view! {
        <main class="shell" data-theme=move || selected_theme.get()>
            <header class="topbar">
                <div>
                    <h1>"SAMSARA"</h1>
                    <p>"Character Sheet Manager"</p>
                </div>
                <label class="level level-control">
                    <span>"NÍVEL"</span>
                    <input
                        type="number"
                        min="1"
                        max="10"
                        aria-label="Nível do personagem"
                        prop:value=move || sheet.get().level.to_string()
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                sheet.update(|s| s.level = v.clamp(1, 10));
                            }
                        }
                    />
                </label>
            </header>

            <nav class="tabs" aria-label="Seções da ficha">
                <button class:active=move || tab.get() == 0 on:click=move |_| tab.set(0)>
                    "Personagem"
                </button>
                <button class:active=move || tab.get() == 1 on:click=move |_| tab.set(1)>
                    "Atributos"
                </button>
                <button class:active=move || tab.get() == 2 on:click=move |_| tab.set(2)>
                    "Anotações"
                </button>
                <Show when=move || is_master>
                    <button class:active=move || tab.get() == 3 on:click=move |_| {
                        viewing_as_master.set(false);
                        tab.set(3);
                        master_loading.set(true);
                        master_error.set(String::new());
                        let current_session = session.get();
                        spawn_local(async move {
                            match cloud::list_all_characters(&current_session).await {
                                Ok(characters) => {
                                    let mut cards = Vec::new();
                                    for character in characters {
                                        let portrait = match character.portrait_path.as_deref() {
                                            Some(path) => cloud::portrait_signed_url(&current_session, path).await.ok(),
                                            None => None,
                                        };
                                        cards.push((character, portrait));
                                    }
                                    master_characters.set(cards);
                                }
                                Err(error) => master_error.set(error),
                            }
                            master_loading.set(false);
                        });
                    }>
                        "Mestre"
                    </button>
                </Show>
            </nav>

            <section class="content">
                <Show when=move || tab.get() == 3 && is_master>
                    <section class="master-dashboard">
                        <h2>"Painel do Mestre"</h2>
                        <Show when=move || master_loading.get()><p>"Carregando fichas..."</p></Show>
                        <Show when=move || !master_error.get().is_empty()>
                            <p class="auth-error">{move || master_error.get()}</p>
                        </Show>
                        <div class="master-character-list">
                            {move || master_characters.get().into_iter().map(|(character, portrait)| {
                                let selected = character.clone();
                                let delete_target = character.clone();
                                let card_id = character.id.clone();
                                let values = &character.sheet_data;
                                let pv = values.get("pv").and_then(|v| v.as_i64()).unwrap_or(0);
                                let san = values.get("san").and_then(|v| v.as_i64()).unwrap_or(0);
                                let pm = values.get("pm").and_then(|v| v.as_i64()).unwrap_or(0);
                                view! {
                                    <article class="master-character-card">
                                        <div class="master-character-portrait">
                                            {if let Some(url) = portrait {
                                                view! { <img src=url alt="Retrato do personagem"/> }.into_any()
                                            } else {
                                                view! { <span>"Sem retrato"</span> }.into_any()
                                            }}
                                        </div>
                                        <div class="master-character-info">
                                            <h3>{character.name.clone()}</h3>
                                            <p>{format!("PV: {} • SAN: {} • PM: {}", pv, san, pm)}</p>
                                        </div>
                                        <div class="master-character-actions">
                                            <button type="button" on:click=move |_| {
                                                let current = selected.clone();
                                                let loaded = serde_json::from_value::<Sheet>(current.sheet_data.clone()).unwrap_or_default();
                                                sheet.set(loaded);
                                                character_id.set(current.id.clone());
                                                portrait_url.set(None);
                                                viewing_as_master.set(true);
                                                tab.set(0);
                                                if let Some(path) = current.portrait_path.clone() {
                                                    let auth_session = session.get();
                                                    spawn_local(async move {
                                                        if let Ok(url) = cloud::portrait_signed_url(&auth_session, &path).await {
                                                            portrait_url.set(Some(url));
                                                        }
                                                    });
                                                }
                                                save_status.set("Ficha aberta em modo Mestre.".to_string());
                                            }>"Abrir ficha"</button>
                                            <Show when=move || master_delete_pending.get() == card_id>
                                                <button type="button" class="danger" on:click=move |_| {
                                                    let auth_session = session.get();
                                                    let target = delete_target.clone();
                                                    spawn_local(async move {
                                                        match cloud::delete_character_as_master(&auth_session, &target).await {
                                                            Ok(()) => {
                                                                master_characters.update(|items| items.retain(|(item, _)| item.id != target.id));
                                                                master_delete_pending.set(String::new());
                                                            }
                                                            Err(error) => master_error.set(error),
                                                        }
                                                    });
                                                }>"Confirmar"</button>
                                            </Show>
                                            <button type="button" class="danger" on:click=move |_| master_delete_pending.set(card_id.clone())>"Excluir"</button>
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                </Show>
                <Show when=move || tab.get() == 0>
                    <div class="character-profile-layout">
                        <div class="portrait-card">
                            <label
                                class="portrait-upload"
                                class:uploading=move || portrait_uploading.get()
                                title="Adicionar ou trocar retrato"
                            >
                                <Show when=move || portrait_url.get().is_some() fallback=|| view! {
                                    <div class="portrait-placeholder"><span>"Sem imagem"</span></div>
                                }>
                                    <img class="character-portrait" src=move || portrait_url.get().unwrap_or_default() alt="Retrato do personagem" />
                                </Show>
                                <span class="portrait-upload-icon" aria-hidden="true">
                                    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M4 7h3l1.5-2h7L17 7h3v12H4z"/>
                                        <circle cx="12" cy="13" r="3.2"/>
                                        <path d="M18 3v4M16 5h4"/>
                                    </svg>
                                </span>
                                <input
                                    class="portrait-file-input"
                                    type="file"
                                    accept="image/jpeg,image/png,image/webp"
                                    disabled=move || portrait_uploading.get()
                                    on:change=move |ev| {
                                        handle_portrait_change(
                                            ev,
                                            session.get(),
                                            character_id.get(),
                                            sheet.clone(),
                                            save_status.clone(),
                                            portrait_url.clone(),
                                            portrait_uploading.clone(),
                                        );
                                    }
                                />
                            </label>
                        </div>
                        <div class="form-grid character-profile-fields">
                            <label>
                                "Nome"
                                <input prop:value=move || sheet.get().name.clone() on:input=move |ev| sheet.update(|s| s.name = event_target_value(&ev)) />
                            </label>
                            <label>
                                "Raça"
                                <select prop:value=move || sheet.get().race.clone() on:change=move |ev| {
                                    let selected = event_target_value(&ev);
                                    sheet.update(|s| {
                                        s.race = selected.clone();
                                        let opts = specialization_options_for_race(&selected);
                                        if opts.is_empty() { s.specialization.clear(); }
                                        else if !opts.iter().any(|opt| *opt == s.specialization.as_str()) {
                                            s.specialization = opts[0].to_string();
                                        }
                                    });
                                }>
                                    <option value="" selected=move || sheet.get().race.is_empty()>"Selecione"</option>
                                    <option value="Humano" selected=move || sheet.get().race == "Humano">"Humano"</option>
                                    <option value="Pesadelo" selected=move || sheet.get().race == "Pesadelo">"Pesadelo"</option>
                                    <option value="Áureo" selected=move || sheet.get().race == "Áureo">"Áureo"</option>
                                </select>
                            </label>
                            <label>
                                "Especialização"
                                <select prop:value=move || sheet.get().specialization.clone() on:change=move |ev| sheet.update(|s| s.specialization = event_target_value(&ev))>
                                    <option value="" selected=move || sheet.get().specialization.is_empty()>"Selecione"</option>
                                    {move || specialization_options_for_race(&sheet.get().race).into_iter().map(|opt| {
                                        let selected = sheet.get().specialization == opt;
                                        view! { <option value={opt} selected={selected}>{opt}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                            <label>
                                "Comunidade"
                                <select prop:value=move || sheet.get().community.clone() on:change=move |ev| sheet.update(|s| s.community = event_target_value(&ev))>
                                    <option value="" selected=move || sheet.get().community.is_empty()>"Selecione"</option>
                                    <option value="Pântano" selected=move || sheet.get().community == "Pântano">"Pântano"</option>
                                    <option value="Tundra" selected=move || sheet.get().community == "Tundra">"Tundra"</option>
                                    <option value="Cidade" selected=move || sheet.get().community == "Cidade">"Cidade"</option>
                                    <option value="Nômade" selected=move || sheet.get().community == "Nômade">"Nômade"</option>
                                    <option value="Deserto" selected=move || sheet.get().community == "Deserto">"Deserto"</option>
                                </select>
                            </label>
                            <label>
                                "Classe"
                                <select prop:value=move || sheet.get().class_name.clone() on:change=move |ev| sheet.update(|s| s.class_name = event_target_value(&ev))>
                                    <option value="" selected=move || sheet.get().class_name.is_empty()>"Selecione"</option>
                                    <option value="Médico" selected=move || sheet.get().class_name == "Médico">"Médico"</option>
                                    <option value="Artífice" selected=move || sheet.get().class_name == "Artífice">"Artífice"</option>
                                    <option value="Engenheiro" selected=move || sheet.get().class_name == "Engenheiro">"Engenheiro"</option>
                                    <option value="Magus" selected=move || sheet.get().class_name == "Magus">"Magus"</option>
                                    <option value="Cozinheiro" selected=move || sheet.get().class_name == "Cozinheiro">"Cozinheiro"</option>
                                    <option value="Rushdown" selected=move || sheet.get().class_name == "Rushdown">"Rushdown"</option>
                                </select>
                            </label>
                        </div>
                    </div>

                    <h2>"Atributos"</h2>
                    <div class="resource-heading">
                        <h3>"Recursos"</h3>
                    </div>
                    <div class="resource-grid">
                        <label class="resource-card">
                            "PV"
                            <input type="number" aria-label="PV atual" prop:value=move || sheet.get().pv.to_string() on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.pv = v); }
                            } />
                            <input class="resource-max-input" type="number" min="0" disabled=move || !resource_editing.get() prop:value=move || {
                                let s = sheet.get();
                                if s.pv_max > 0 { s.pv_max } else { s.pv }.to_string()
                            } on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.pv_max = v.max(0)); }
                            } />
                        </label>
                        <label class="resource-card">
                            "SAN"
                            <input type="number" aria-label="SAN atual" prop:value=move || sheet.get().san.to_string() on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.san = v); }
                            } />
                            <input class="resource-max-input" type="number" min="0" disabled=move || !resource_editing.get() prop:value=move || {
                                let s = sheet.get();
                                if s.san_max > 0 { s.san_max } else { s.san }.to_string()
                            } on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.san_max = v.max(0)); }
                            } />
                        </label>
                        <label class="resource-card">
                            "PM"
                            <input type="number" aria-label="PM atual" prop:value=move || sheet.get().pm.to_string() on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.pm = v); }
                            } />
                            <input class="resource-max-input" type="number" min="0" disabled=move || !resource_editing.get() prop:value=move || {
                                let s = sheet.get();
                                if s.pm_max > 0 { s.pm_max } else { s.pm }.to_string()
                            } on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.pm_max = v.max(0)); }
                            } />
                        </label>
                    </div>

                    <h3>"Atributos"</h3>
                    <div class="stat-grid">
                        <div class="stat"><strong>"INT"</strong><input aria-label="Atributo INT" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[0].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[0] = v); } } /></div>
                        <div class="stat"><strong>"FIS"</strong><input aria-label="Atributo FIS" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[1].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[1] = v); } } /></div>
                        <div class="stat"><strong>"DOM"</strong><input aria-label="Atributo DOM" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[2].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[2] = v); } } /></div>
                        <div class="stat"><strong>"VEL"</strong><input aria-label="Atributo VEL" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[3].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[3] = v); } } /></div>
                        <div class="stat"><strong>"HAB"</strong><input aria-label="Atributo HAB" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[4].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[4] = v); } } /></div>
                    </div>

                    <section class="test-advantages-section">
                        <h3>"Vantagens"</h3>
                        <p class="hint">"Defina quantas vantagens o personagem possui em cada tipo de teste. uma vantagem equivale a +1d4 no teste."</p>
                        <div class="test-advantages-grid">
                            {move || {
                                const TESTS: [(&str, &str); 16] = [
                                    ("Desviar", "VEL + HAB"),
                                    ("Sobrevivência", "FIS + INT / HAB"),
                                    ("Social", "DOM"),
                                    ("Combate", "FIS + HAB / VEL"),
                                    ("Escapar", "VEL + FIS"),
                                    ("Pontaria", "HAB"),
                                    ("Inventar", "HAB + INT"),
                                    ("Medicina", "DOM + INT"),
                                    ("Investigação", "INT"),
                                    ("Elusão", "HAB"),
                                    ("Capacidade", "FIS"),
                                    ("Acrobacia", "VEL + HAB"),
                                    ("Iniciativa", "VEL"),
                                    ("Percepção", "HAB + DOM"),
                                    ("Resistir", "FIS + DOM"),
                                    ("Dharma", "DOM"),
                                ];
                                TESTS.into_iter().enumerate().map(|(index, (name, formula))| {
                                    let test_name = name.to_string();
                                    view! {
                                        <label class="test-advantage-card">
                                            <span class="test-advantage-name">{test_name}</span>
                                            <span class="test-advantage-formula">{formula}</span>
                                            <input
                                                type="number"
                                                min="0"
                                                step="1"
                                                aria-label=format!("Vantagens em {}", name)
                                                disabled=move || !resource_editing.get()
                                                prop:value=move || sheet.get().test_advantages.get(index).copied().unwrap_or(0).to_string()
                                                on:input=move |ev| {
                                                    if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                                                        sheet.update(|s| {
                                                            if s.test_advantages.len() < 16 {
                                                                s.test_advantages.resize(16, 0);
                                                            }
                                                            s.test_advantages[index] = value.max(0);
                                                        });
                                                    }
                                                }
                                            />
                                        </label>
                                    }
                                }).collect_view()
                            }}
                        </div>
                    </section>



                </Show>

                <Show when=move || tab.get() == 1>
                    <h3>"Habilidades"</h3>
                    <div class="skill-field">
                        <div class="skill-form">
                            <Show when=move || skill_form_open.get()>
                                <input
                                    class="skill-input"
                                    placeholder="Nome da habilidade"
                                    prop:value=move || skill_name.get()
                                    on:input=move |ev| skill_name.set(event_target_value(&ev))
                                />
                                <div class="skill-description-row">
                                    <textarea
                                        class="skill-input skill-description-input"
                                        rows="3"
                                        placeholder="Descrição"
                                        prop:value=move || skill_description.get()
                                        on:input=move |ev| skill_description.set(event_target_value(&ev))
                                    ></textarea>
                                </div>
                            </Show>
                        </div>
                        <button type="button" class="skill-add" on:click=submit_skill>
                            {move || if skill_form_open.get() { "Salvar" } else { "Adicionar" }}
                        </button>
                        <div class="skill-list">{render_skill_list}</div>
                    </div>

                </Show>

                <Show when=move || tab.get() == 2>
                    <div class="notes-panel">
                        <h2>"Anotações"</h2>
                        <p class="hint">"Escreva.."</p>
                        <textarea class="notes" rows="23" placeholder="Escreva aqui..." prop:value=move || sheet.get().notes.clone() on:input=move |ev| sheet.update(|s| s.notes = event_target_value(&ev))></textarea>
                    </div>
                    <inventory::Inventory sheet=sheet />
                </Show>
            </section>

            <div class="theme-controls">
                <Show when=move || theme_picker_open.get()>
                    <section class="theme-picker" role="dialog" aria-label="Escolher tema de cores">
                        <div class="theme-picker-heading">
                            <span>"Paleta de cores"</span>
                            <button type="button" class="theme-picker-close" aria-label="Fechar seletor de tema" on:click=move |_| theme_picker_open.set(false)>
                                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg>
                            </button>
                        </div>
                        <div class="theme-options">
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "green" on:click=move |_| { selected_theme.set("green".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-green"></span><span>"Verde"</span>
                                <Show when=move || selected_theme.get() == "green"><span class="theme-check">"✓"</span></Show>
                            </button>
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "blue" on:click=move |_| { selected_theme.set("blue".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-blue"></span><span>"Azul"</span>
                                <Show when=move || selected_theme.get() == "blue"><span class="theme-check">"✓"</span></Show>
                            </button>
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "red" on:click=move |_| { selected_theme.set("red".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-red"></span><span>"Vermelho"</span>
                                <Show when=move || selected_theme.get() == "red"><span class="theme-check">"✓"</span></Show>
                            </button>
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "yellow" on:click=move |_| { selected_theme.set("yellow".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-yellow"></span><span>"Amarelo"</span>
                                <Show when=move || selected_theme.get() == "yellow"><span class="theme-check">"✓"</span></Show>
                            </button>
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "purple" on:click=move |_| { selected_theme.set("purple".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-purple"></span><span>"Roxo"</span>
                                <Show when=move || selected_theme.get() == "purple"><span class="theme-check">"✓"</span></Show>
                            </button>
                            <button type="button" class="theme-option" class:active=move || selected_theme.get() == "mono" on:click=move |_| { selected_theme.set("mono".to_string()); theme_picker_open.set(false); }>
                                <span class="theme-swatch theme-swatch-mono"></span><span>"Monocromático"</span>
                                <Show when=move || selected_theme.get() == "mono"><span class="theme-check">"✓"</span></Show>
                            </button>
                        </div>
                    </section>
                </Show>
                <button type="button" class="theme-toggle" aria-label="Personalizar tema de cores" title="Personalizar tema" on:click=move |_| theme_picker_open.update(|open| *open = !*open)>
                    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                        <path d="M20.2 15.1A8.5 8.5 0 0 1 8.9 3.8 8.7 8.7 0 1 0 20.2 15.1Z"/>
                        <path d="M16.5 3.5v3M15 5h3"/>
                    </svg>
                </button>
            </div>

            <footer class="account-footer">
                <div class="account-footer-actions">
                    <Show when=move || tab.get() == 0 && !resource_editing.get()>
                        <button
                            type="button"
                            class="resource-edit"
                            disabled=move || saving.get()
                            on:click=move |_| {
                                sheet.update(|s| {
                                    if s.pv_max <= 0 { s.pv_max = s.pv; }
                                    if s.san_max <= 0 { s.san_max = s.san; }
                                    if s.pm_max <= 0 { s.pm_max = s.pm; }
                                });
                                resource_editing.set(true);
                                save_status.set("Edite os valores máximos e os atributos e clique em Salvar ficha.".to_string());
                            }
                        >
                            "Editar atributos"
                        </button>
                    </Show>

                    <Show when=move || viewing_as_master.get()>
                        <button type="button" class="resource-edit" on:click=move |_| {
                            viewing_as_master.set(false);
                            tab.set(3);
                            save_status.set("Retornou ao painel do Mestre.".to_string());
                        }>"Voltar ao Mestre"</button>
                    </Show>
                    <button
                        type="button"
                        class="resource-edit"
                        disabled=move || saving.get()
                        on:click=move |_| {
                            let current_sheet = sheet.get();
                            let current_session = session.get();
                            let current_id = character_id.get();
                            let name = current_sheet.name.clone();
                            let data = match serde_json::to_value(&current_sheet) {
                                Ok(value) => value,
                                Err(error) => { save_status.set(format!("Erro: {}", error)); return; }
                            };
                            saving.set(true);
                            save_status.set("Salvando...".to_string());
                            spawn_local(async move {
                                let result = if viewing_as_master.get_untracked() {
                                    cloud::update_character_as_master(&current_session, &current_id, &name, current_sheet.portrait_path.as_deref(), data).await
                                } else {
                                    cloud::update_character(&current_session, &current_id, &name, current_sheet.portrait_path.as_deref(), data).await
                                };
                                match result {
                                    Ok(()) => {
                                        save_status.set("Ficha salva na nuvem.".to_string());
                                        resource_editing.set(false);
                                    }
                                    Err(error) => save_status.set(format!("Erro ao salvar: {}", error)),
                                }
                                saving.set(false);
                            });
                        }
                    >
                        {move || if saving.get() { "Salvando..." } else { "Salvar ficha" }}
                    </button>

                    <button
                        type="button"
                        class="resource-edit"
                        on:click=move |_| on_logout.run(())
                    >
                        "Sair da conta"
                    </button>
                </div>
                <span>{move || save_status.get()}</span>
            </footer>
        </main>
    }
}

#[component]
fn App() -> impl IntoView {
    let account = RwSignal::new(None::<(auth::AuthSession, Sheet, String)>);
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let register_mode = RwSignal::new(false);
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error_message = RwSignal::new(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            if auth::load_session().is_some() {
                match auth::refresh_session().await {
                    Ok(session) => match prepare_account(session).await {
                        Ok(data) => account.set(Some(data)),
                        Err(error) => error_message.set(error),
                    },
                    Err(error) => error_message.set(error),
                }
            }
            loading.set(false);
        });
    });

    let submit_auth = move |_| {
        if busy.get_untracked() { return; }
        let user = username.get_untracked();
        let pass = password.get_untracked();
        let registering = register_mode.get_untracked();
        busy.set(true);
        error_message.set(String::new());

        spawn_local(async move {
            let result = if registering { auth::register(&user, &pass).await } else { auth::login(&user, &pass).await };
            match result {
                Ok(session) => match prepare_account(session).await {
                    Ok(data) => account.set(Some(data)),
                    Err(error) => error_message.set(error),
                },
                Err(error) => error_message.set(error),
            }
            busy.set(false);
        });
    };

    let logout = Callback::new(move |_| {
        spawn_local(async move {
            let _ = auth::logout().await;
            account.set(None);
            username.set(String::new());
            password.set(String::new());
            error_message.set(String::new());
        });
    });

    view! {
        <Show
            when=move || !loading.get()
            fallback=|| view! { <main class="auth-screen"><p>"Carregando..."</p></main> }
        >
            {move || match account.get() {
                Some((session, initial_sheet, character_id)) => view! {
                    <SheetApp initial_sheet=initial_sheet session=session character_id=character_id on_logout=logout />
                }.into_any(),
                None => view! {
                    <main class="auth-screen">
                        <section class="auth-card">
                            <h1>"SAMSARA"</h1>
                            <p>"Character Sheet Manager"</p>
                            <h2>{move || if register_mode.get() { "Criar conta" } else { "Entrar" }}</h2>
                            <label>
                                "Nome de usuário"
                                <input type="text" placeholder="Seu usuário" autocomplete="username" prop:value=move || username.get() on:input=move |ev| username.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                "Senha"
                                <input type="password" placeholder="Sua senha" autocomplete=move || if register_mode.get() { "new-password" } else { "current-password" } prop:value=move || password.get() on:input=move |ev| password.set(event_target_value(&ev)) />
                            </label>
                            <p class="auth-error">{move || error_message.get()}</p>
                            <button type="button" disabled=move || busy.get() on:click=submit_auth>
                                {move || if busy.get() { "Aguarde..." } else if register_mode.get() { "Cadastrar" } else { "Entrar" }}
                            </button>
                            <button type="button" class="auth-switch" on:click=move |_| {
                                register_mode.update(|v| *v = !*v);
                                error_message.set(String::new());
                            }>
                                {move || if register_mode.get() { "Já tenho uma conta" } else { "Criar uma conta" }}
                            </button>
                        </section>
                    </main>
                }.into_any(),
            }}
        </Show>
    }
}

fn main() {
    mount_to_body(App);
}