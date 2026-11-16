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
    let dragged_skill = RwSignal::new(None::<String>);
    let expanded_skill_folders = RwSignal::new(Vec::<String>::new());
    let portrait_url = RwSignal::new(None::<String>);
    let portrait_uploading = RwSignal::new(false);

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
                .find(|item| item.children.iter().any(|child| child.name == source_name)
                    && item.children.iter().any(|child| child.name == target_name))
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
        dragged_skill.set(None);
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
                                        dragged_skill.set(Some(entry_name_for_drag.clone()));
                                    }
                                    on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                                    on:drop=move |ev: web_sys::DragEvent| drop_skill_on(entry_name_for_drop.clone(), ev)
                                >
                                    <div class="item-title">
                                        <strong>{entry.name}</strong>
                                        <button
                                            type="button"
                                            class="remove"
                                            on:click=move |_| remove_skill(entry_name_for_remove.clone())
                                        >"Remover"</button>
                                    </div>
                                    <p>{description}</p>
                                    <span class="skill-drag-hint">"Arraste sobre outra habilidade para agrupar"</span>
                                </div>
                            }.into_any()
                        } else {
                            let folder_name_for_toggle = entry.name.clone();
                            let folder_name_for_drop = entry.name.clone();
                            let folder_name_for_description = entry.name.clone();
                            let folder_name_for_remove = entry.name.clone();
                            let is_expanded = expanded_skill_folders.get().contains(&entry.name);
                            let folder_description = entry.description.clone();
                            let children = entry.children.clone();
                            view! {
                                <div
                                    class="item skill-folder"
                                    on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                                    on:drop=move |ev: web_sys::DragEvent| drop_skill_on(folder_name_for_drop.clone(), ev)
                                >
                                    <div class="item-title skill-folder-heading">
                                        <button
                                            type="button"
                                            class="skill-folder-toggle"
                                            aria-label="Expandir ou recolher pasta"
                                            on:click=move |_| expanded_skill_folders.update(|expanded| {
                                                if let Some(index) = expanded.iter().position(|name| name == &folder_name_for_toggle) {
                                                    expanded.remove(index);
                                                } else {
                                                    expanded.push(folder_name_for_toggle.clone());
                                                }
                                            })
                                        >
                                            {if is_expanded { "▾" } else { "▸" }}
                                        </button>
                                        <strong>{entry.name}</strong>
                                        <span class="skill-folder-count">{format!("{} habilidades", children.len())}</span>
                                        <button
                                            type="button"
                                            class="remove"
                                            on:click=move |_| remove_skill(folder_name_for_remove.clone())
                                        >"Remover"</button>
                                    </div>
                                    <textarea
                                        class="skill-folder-description"
                                        rows="2"
                                        placeholder="Descrição da pasta"
                                        prop:value=folder_description
                                        on:input=move |ev| {
                                            let value = event_target_value(&ev);
                                            let mut current = skill_items();
                                            if let Some(folder) = current.iter_mut().find(|item| item.name == folder_name_for_description) {
                                                folder.description = value;
                                                save_skill_items(current);
                                            }
                                        }
                                    ></textarea>
                                    {if is_expanded {
                                        view! {
                                            <div class="skill-folder-contents">
                                                {children.into_iter().map(|child| {
                                                    let child_name_for_remove = child.name.clone();
                                                    let child_description = if child.description.is_empty() {
                                                        "Nenhuma descrição registrada.".to_string()
                                                    } else {
                                                        child.description.clone()
                                                    };
                                                    view! {
                                                        <div
                                                            class="skill-folder-child"
                                                        >
                                                            <div class="item-title">
                                                                <strong>{child.name}</strong>
                                                                <button type="button" class="remove" on:click=move |_| remove_skill(child_name_for_remove.clone())>"Remover"</button>
                                                            </div>
                                                            <p>{child_description}</p>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <p class="skill-folder-summary">{if entry.description.is_empty() { "Pasta de habilidades".to_string() } else { entry.description.clone() }}</p> }.into_any()
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
        <main class="shell">
            <header class="topbar">
                <div>
                    <h1>"SAMSARA"</h1>
                    <p>"Character Sheet Manager"</p>
                </div>
                <span class="level">"NÍVEL " {move || sheet.get().level}</span>
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
            </nav>

            <section class="content">
                <Show when=move || tab.get() == 0>
                    <div class="character-profile-layout">
                        <div class="portrait-card">
                            <Show when=move || portrait_url.get().is_some() fallback=|| view! {
                                <div class="portrait-placeholder"><span>"Sem imagem"</span></div>
                            }>
                                <img class="character-portrait" src=move || portrait_url.get().unwrap_or_default() alt="Retrato do personagem" />
                            </Show>
                            <label class="portrait-upload">
                                {move || if portrait_uploading.get() { "Enviando..." } else if portrait_url.get().is_some() { "Trocar imagem" } else { "Adicionar imagem" }}
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
                            <label>
                                "Nível"
                                <input type="number" min="1" max="10" prop:value=move || sheet.get().level.to_string() on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                        sheet.update(|s| s.level = v.clamp(1, 10));
                                    }
                                } />
                            </label>
                        </div>
                    </div>

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

                <Show when=move || tab.get() == 1>
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

                <Show when=move || tab.get() == 2>
                    <div class="notes-panel">
                        <h2>"Anotações"</h2>
                        <p class="hint">"Escreva.."</p>
                        <textarea class="notes" rows="23" placeholder="Escreva aqui..." prop:value=move || sheet.get().notes.clone() on:input=move |ev| sheet.update(|s| s.notes = event_target_value(&ev))></textarea>
                    </div>
                    <inventory::Inventory sheet=sheet />
                </Show>
            </section>

            <footer class="account-footer">
                <div class="account-footer-actions">
                    <Show when=move || tab.get() == 1 && !resource_editing.get()>
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
                                let result = cloud::update_character(&current_session, &current_id, &name, current_sheet.portrait_path.as_deref(), data).await;
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