use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, closure::Closure};
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
    #[serde(default)]
    conditions: Vec<ConditionEntry>,
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
            conditions: default_conditions(),
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
    if sheet.conditions.is_empty() {
        sheet.conditions = default_conditions();
    }
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

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct ConditionEntry {
    name: String,
    description: String,
}

fn default_conditions() -> Vec<ConditionEntry> {
    vec![
        ConditionEntry { name: "Enjoado".into(), description: "Seu personagem pode realizar apenas uma destas opções por turno: uma ação padrão ou uma ação de movimento. Não pode realizar ambas no mesmo turno.".into() },
        ConditionEntry { name: "Silenciado".into(), description: "Seu personagem não poderá realizar nenhuma ação que envolva custo de PM.".into() },
        ConditionEntry { name: "Atordoado".into(), description: "Seu personagem poderá realizar apenas ações de movimento, com metade da distância comum.".into() },
        ConditionEntry { name: "Esfolado".into(), description: "Toda vez que realizar um ataque, seu personagem deverá fazer um teste de Resistir, com DT definida pelo mestre. Se passar, realiza o ataque normalmente. Se falhar, fica desmaiado por 1d4 rodadas.".into() },
    ]
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

fn persist_combat_state(
    session: auth::AuthSession,
    state: cloud::CombatState,
    state_signal: RwSignal<Option<cloud::CombatState>>,
    error_signal: RwSignal<String>,
) {
    spawn_local(async move {
        match cloud::save_combat_state(&session, &state).await {
            Ok(()) => {
                state_signal.set(Some(state));
                error_signal.set(String::new());
            }
            Err(error) => error_signal.set(error),
        }
    });
}

#[component]
fn SheetApp(
    initial_sheet: Sheet,
    session: auth::AuthSession,
    character_id: String,
    on_logout: Callback<()>,
) -> impl IntoView {
    let initial_portrait_path = initial_sheet.portrait_path.clone();
    let own_sheet = StoredValue::new(initial_sheet.clone());
    let own_character_id = StoredValue::new(character_id.clone());
    let own_portrait_path = StoredValue::new(initial_portrait_path.clone());
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
    let master_mode = RwSignal::new(false);
    let combat_setup_open = RwSignal::new(false);
    let combat_selected = RwSignal::new(Vec::<String>::new());
    let combat_state = RwSignal::new(None::<cloud::CombatState>);
    let combat_error = RwSignal::new(String::new());
    let turn_popup_open = RwSignal::new(false);
    let turn_popup_key = RwSignal::new(String::new());
    let turn_popup_participant = RwSignal::new(None::<cloud::CombatParticipant>);
    let expanded_turn_player = RwSignal::new(String::new());
    let conditions_library = RwSignal::new(default_conditions());
    let condition_name = RwSignal::new(String::new());
    let condition_description = RwSignal::new(String::new());
    let condition_form_open = RwSignal::new(false);

    Effect::new(move |_| {
        let storage_key = format!("samsara-color-theme-{}", character_id.get());
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item(&storage_key, &selected_theme.get());
        }
    });

    if let Some(path) = own_portrait_path.clone().get_value() {
        let current_session = session.get();
        spawn_local(async move {
            match cloud::portrait_signed_url(&current_session, &path).await {
                Ok(url) => portrait_url.set(Some(url)),
                Err(error) => save_status.set(format!("Não foi possível carregar o retrato: {}", error)),
            }
        });
    }

    Effect::new(move |_| save_sheet(&sheet.get()));

    let load_master_roster = move || {
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
                    let mut library = Vec::<ConditionEntry>::new();
                    for (character, _) in &cards {
                        let parsed = character.sheet_data.get("conditions")
                            .and_then(|value| serde_json::from_value::<Vec<ConditionEntry>>(value.clone()).ok())
                            .filter(|items| !items.is_empty())
                            .unwrap_or_else(default_conditions);
                        for condition in parsed {
                            if !library.iter().any(|existing| existing.name.eq_ignore_ascii_case(&condition.name)) {
                                library.push(condition);
                            }
                        }
                    }
                    library.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                    conditions_library.set(library);
                    master_characters.set(cards);
                }
                Err(error) => master_error.set(error),
            }
            master_loading.set(false);
        });
    };

    let restore_own_sheet = move || {
        if viewing_as_master.get_untracked() {
            sheet.set(own_sheet.clone().get_value());
            character_id.set(own_character_id.clone().get_value());
            portrait_url.set(None);
            if let Some(path) = own_portrait_path.get_value() {
                let current_session = session.get();
                spawn_local(async move {
                    if let Ok(url) = cloud::portrait_signed_url(&current_session, &path).await {
                        portrait_url.set(Some(url));
                    }
                });
            }
        }
        viewing_as_master.set(false);
    };

    let persist_combat = move |state: cloud::CombatState| {
        let current_session = session.get();
        let state_signal = combat_state.clone();
        let error_signal = combat_error.clone();
        spawn_local(async move {
            match cloud::save_combat_state(&current_session, &state).await {
                Ok(()) => {
                    state_signal.set(Some(state));
                    error_signal.set(String::new());
                }
                Err(error) => error_signal.set(error),
            }
        });
    };

    // Os jogadores consultam o estado compartilhado periodicamente para receber o aviso.
    let poll_session = session.clone();
    let poll_character_id = character_id.clone();
    let poll_master_mode = master_mode.clone();
    let poll_viewing_as_master = viewing_as_master.clone();
    let poll_popup = turn_popup_open.clone();
    let poll_key = turn_popup_key.clone();
    let poll_tab = tab.clone();
    let poll_conditions = conditions_library.clone();
    let poll_is_master = is_master;
    let poll_callback = Closure::wrap(Box::new(move || {
        if poll_is_master {
            if poll_master_mode.get_untracked() && poll_tab.get_untracked() == 4 {
                let current_session = poll_session.get_untracked();
                let library_signal = poll_conditions.clone();
                spawn_local(async move {
                    if let Ok(characters) = cloud::list_all_characters(&current_session).await {
                        let mut library = Vec::<ConditionEntry>::new();
                        for character in characters {
                            let parsed = character.sheet_data.get("conditions")
                                .and_then(|value| serde_json::from_value::<Vec<ConditionEntry>>(value.clone()).ok())
                                .filter(|items| !items.is_empty())
                                .unwrap_or_else(default_conditions);
                            for condition in parsed {
                                if !library.iter().any(|existing| existing.name.eq_ignore_ascii_case(&condition.name)) {
                                    library.push(condition);
                                }
                            }
                        }
                        library.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        library_signal.set(library);
                    }
                });
            }
            return;
        }
        if poll_master_mode.get_untracked() || poll_viewing_as_master.get_untracked() {
            return;
        }
        let current_session = poll_session.get_untracked();
        let current_character_id = poll_character_id.get_untracked();
        let popup = poll_popup.clone();
        let popup_key = poll_key.clone();
        spawn_local(async move {
            match cloud::load_combat_state(&current_session).await {
                Ok(Some(state)) if state.is_active => {
                    if let Some(current) = state.turn_order.get(state.current_index) {
                        if current.character_id == current_character_id {
                            let key = format!("{}:{}:{}", state.round, state.current_index, current.character_id);
                            turn_popup_participant.set(Some(current.clone()));
                            if popup_key.get_untracked() != key {
                                popup_key.set(key);
                                popup.set(true);
                            }
                        } else {
                            popup.set(false);
                        }
                    } else { popup.set(false); }
                }
                _ => popup.set(false),
            }
        });
    }) as Box<dyn FnMut()>);
    let poll_window = web_sys::window();
    let poll_interval = poll_window.as_ref().and_then(|window| {
        window.set_interval_with_callback_and_timeout_and_arguments_0(
            poll_callback.as_ref().unchecked_ref(), 2500
        ).ok()
    });
    if let Some(interval) = poll_interval {
        on_cleanup(move || {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(interval);
            }
        });
    }
    poll_callback.forget();

    let skill_items = move || load_skill_items(&sheet.get().abilities);

    let save_skill_items = move |items: Vec<SkillEntry>| {
        sheet.update(|s| {
            s.abilities = serde_json::to_string(&items).unwrap_or_default();
        });
    };

    let add_condition = move |_| {
        let name = condition_name.get().trim().to_string();
        let description = condition_description.get().trim().to_string();
        if name.is_empty() || description.is_empty() {
            return;
        }
        let mut current = sheet.get_untracked();
        if current.conditions.iter().any(|item| item.name.eq_ignore_ascii_case(&name)) {
            return;
        }
        current.conditions.push(ConditionEntry { name, description });
        current.conditions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        sheet.set(current.clone());
        let current_session = session.get_untracked();
        let current_id = character_id.get_untracked();
        let is_master_view = viewing_as_master.get_untracked();
        let data = match serde_json::to_value(&current) {
            Ok(value) => value,
            Err(error) => {
                save_status.set(format!("Erro ao preparar condição: {}", error));
                return;
            }
        };
        let current_name = current.name.clone();
        let portrait_path = current.portrait_path.clone();
        save_status.set("Salvando nova condição...".to_string());
        spawn_local(async move {
            let result = if is_master_view {
                cloud::update_character_as_master(&current_session, &current_id, &current_name, portrait_path.as_deref(), data).await
            } else {
                cloud::update_character(&current_session, &current_id, &current_name, portrait_path.as_deref(), data).await
            };
            match result {
                Ok(()) => save_status.set("Condição adicionada e sincronizada.".to_string()),
                Err(error) => save_status.set(format!("A condição ficou na ficha, mas não foi sincronizada: {}", error)),
            }
        });
        condition_name.set(String::new());
        condition_description.set(String::new());
        condition_form_open.set(false);
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
                                            >
                                                {if is_editing {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                            <path d="m5 12 4 4L19 6"/>
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                            <path d="M12 20h9"/>
                                                            <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"/>
                                                        </svg>
                                                    }.into_any()
                                                }}
                                            </button>
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
                                                            on:change={
                                                                let notes_id = notes_id.clone();
                                                                move |ev| {
                                                                let value = event_target_value(&ev);
                                                                let mut current = skill_items();
                                                                if let Some(folder) = current.iter_mut().find(|item| item.name == folder_name_for_description) {
                                                                    folder.description = value;
                                                                    save_skill_items(current);
                                                                }
                                                            }}
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
                                                                        >
                                                                            {if child_editing {
                                                                                view! {
                                                                                    <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                                                        <path d="m5 12 4 4L19 6"/>
                                                                                    </svg>
                                                                                }.into_any()
                                                                            } else {
                                                                                view! {
                                                                                    <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                                                        <path d="M12 20h9"/>
                                                                                        <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"/>
                                                                                    </svg>
                                                                                }.into_any()
                                                                            }}
                                                                        </button>
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
                    <img class="site-logo" src="logo.png" alt="SAMSARA" />
                    <p>"Character Sheet Manager"</p>
                </div>
                <Show when=move || is_master>
                    <button type="button" class="mode-toggle" class:mode-toggle-active=move || master_mode.get()
                        on:click=move |_| {
                            if !master_mode.get() {
                                restore_own_sheet();
                                master_mode.set(true);
                                resource_editing.set(false);
                                tab.set(3);
                                load_master_roster();
                                let current_session = session.get();
                                spawn_local(async move {
                                    match cloud::load_combat_state(&current_session).await {
                                        Ok(state) => combat_state.set(state),
                                        Err(error) => combat_error.set(error),
                                    }
                                });
                            } else {
                                restore_own_sheet();
                                master_mode.set(false);
                                tab.set(0);
                                resource_editing.set(false);
                            }
                        }>
                        {move || if master_mode.get() { "Modo Player" } else { "Modo Mestre" }}
                    </button>
                </Show>
                <Show when=move || !master_mode.get()>
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
                </Show>

            </header>

            <nav class="tabs" aria-label="Seções">
                <Show when=move || !master_mode.get()>
                    <button class:active=move || tab.get() == 0 on:click=move |_| tab.set(0)>"Personagem"</button>
                    <button class:active=move || tab.get() == 1 on:click=move |_| tab.set(1)>"Habilidades"</button>
                    <button class:active=move || tab.get() == 2 on:click=move |_| tab.set(2)>"Anotações"</button>
                </Show>
                <Show when=move || master_mode.get()>
                    <button class:active=move || tab.get() == 3 on:click=move |_| { tab.set(3); load_master_roster(); }>"Mestre"</button>
                    <button class:active=move || tab.get() == 4 on:click=move |_| {
                        tab.set(4);
                        load_master_roster();
                        let current_session = session.get();
                        spawn_local(async move {
                            match cloud::load_combat_state(&current_session).await {
                                Ok(state) => combat_state.set(state),
                                Err(error) => combat_error.set(error),
                            }
                        });
                    }>"Turnos"</button>
                </Show>
            </nav>

            <section class="content">
                <Show when=move || tab.get() == 3 && master_mode.get() && is_master>
                    <section class="master-dashboard">
                        <h2>"Painel do Mestre"</h2>
                        <Show when=move || master_loading.get()><p>"Carregando fichas..."</p></Show>
                        <Show when=move || !master_error.get().is_empty()>
                            <p class="auth-error">{move || master_error.get()}</p>
                        </Show>
                        <div class="master-character-list">
                            {move || master_characters.get().into_iter().map(|(character, portrait)| {
                                let selected = character.clone();
                                let delete_target = StoredValue::new(character.clone());
                                let card_id = character.id.clone();
                                let show_card_id = card_id.clone();
                                let button_card_id = card_id.clone();
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
                                                master_mode.set(false);
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
                                            <Show when=move || master_delete_pending.get() == show_card_id.clone()>
                                                <button type="button" class="danger" on:click=move |_| {
                                                    let auth_session = session.get();
                                                    let target = delete_target.get_value();
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
                                            <button type="button" class="danger" on:click=move |_| master_delete_pending.set(button_card_id.clone())>"Excluir"</button>
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                </Show>
                <Show when=move || tab.get() == 4 && master_mode.get() && is_master>
                    <section class="turns-dashboard">
                        <h2>"Turnos"</h2>
                        <Show when=move || !combat_error.get().is_empty()>
                            <p class="auth-error">{move || combat_error.get()}</p>
                        </Show>
                        <Show when=move || !combat_state.get().as_ref().map(|state| state.is_active).unwrap_or(false)>
                            <p class="hint">"Organize a ordem de ação dos personagens durante o combate."</p>
                            <button type="button" class="resource-edit" on:click=move |_| {
                                combat_setup_open.set(true);
                                combat_selected.set(Vec::new());
                                load_master_roster();
                            }>"Iniciar combate"</button>
                            <Show when=move || combat_setup_open.get()>
                                <div class="combat-setup">
                                    <h3>"Participantes"</h3>
                                    <p class="hint">"Selecione as fichas que participarão deste combate."</p>
                                    {move || master_characters.get().into_iter().map(|(character, _)| {
                                        let id = character.id.clone();
                                        let checked_id = id.clone();
                                        let change_id = id.clone();
                                        let label = character.name.clone();
                                        view! {
                                            <label class="combat-select-row">
                                                <input type="checkbox"
                                                    prop:checked=move || combat_selected.get().contains(&checked_id)
                                                    on:change=move |ev| {
                                                        let checked = ev.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()).map(|input| input.checked()).unwrap_or(false);
                                                        combat_selected.update(|items| {
                                                            if checked {
                                                                if !items.contains(&change_id) { items.push(change_id.clone()); }
                                                            } else {
                                                                items.retain(|item| item != &change_id);
                                                            }
                                                        });
                                                    }
                                                />
                                                <span>{label}</span>
                                            </label>
                                        }
                                    }).collect_view()}
                                    <div class="combat-actions">
                                        <button type="button" class="resource-edit" on:click=move |_| {
                                            let selected_ids = combat_selected.get();
                                            let participants: Vec<cloud::CombatParticipant> = master_characters.get().into_iter()
                                                .filter(|(character, _)| selected_ids.contains(&character.id))
                                                .map(|(character, _)| cloud::CombatParticipant {
                                                    character_id: character.id,
                                                    name: character.name,
                                                    user_id: character.user_id,
                                                    notes: String::new(),
                                                    condition_name: String::new(),
                                                    condition_description: String::new(),
                                                }).collect();
                                            if participants.is_empty() {
                                                combat_error.set("Selecione pelo menos um personagem.".to_string());
                                                return;
                                            }
                                            let mut user_ids: Vec<String> = participants.iter().map(|p| p.user_id.clone()).collect();
                                            user_ids.sort();
                                            user_ids.dedup();
                                            let state = cloud::CombatState {
                                                id: 1,
                                                is_active: true,
                                                participant_user_ids: user_ids,
                                                turn_order: participants,
                                                current_index: 0,
                                                round: 1,
                                            };
                                            persist_combat_state(session.get(), state, combat_state.clone(), combat_error.clone());
                                            combat_setup_open.set(false);
                                            combat_error.set(String::new());
                                        }>"Começar combate"</button>
                                        <button type="button" class="resource-edit" on:click=move |_| combat_setup_open.set(false)>"Cancelar"</button>
                                    </div>
                                </div>
                            </Show>
                        </Show>
                        <Show when=move || combat_state.get().as_ref().map(|state| state.is_active).unwrap_or(false)>
                            <div class="combat-current-turn">
                                <span>"Turno atual"</span>
                                <strong>{move || combat_state.get().and_then(|state| state.turn_order.get(state.current_index).map(|p| p.name.clone())).unwrap_or_else(|| "—".to_string())}</strong>
                                <small>{move || format!("Rodada {}", combat_state.get().map(|state| state.round).unwrap_or(1))}</small>
                            </div>
                            <h3>"Ordem de ação"</h3>
                            <p class="hint">"Arraste os personagens para reorganizar a ordem. A alteração é sincronizada com os jogadores."</p>
                            <ol class="turn-order-list">
                                {move || combat_state.get().map(|state| state.turn_order.iter().enumerate().map(|(index, participant)| {
                                    let id = participant.character_id.clone();
                                    let drag_id = id.clone();
                                    let drop_id = id.clone();
                                    let expand_id = id.clone();
                                    let expand_label_id = id.clone();
                                    let show_details_id = id.clone();
                                    let notes_id = id.clone();
                                    let condition_id = id.clone();
                                    let is_current = index == state.current_index;
                                    view! {
                                        <li class:current-turn=is_current draggable="true"
                                            on:dragstart=move |ev: web_sys::DragEvent| {
                                                if let Some(data) = ev.data_transfer() {
                                                    let _ = data.set_data("text/plain", &drag_id);
                                                    data.set_effect_allowed("move");
                                                }
                                            }
                                            on:dragover=|ev: web_sys::DragEvent| ev.prevent_default()
                                            on:drop=move |ev: web_sys::DragEvent| {
                                                ev.prevent_default();
                                                let source = ev.data_transfer().and_then(|data| data.get_data("text/plain").ok()).unwrap_or_default();
                                                let Some(mut next) = combat_state.get_untracked() else { return; };
                                                let active_id = next.turn_order.get(next.current_index).map(|p| p.character_id.clone());
                                                if let (Some(from), Some(to)) = (
                                                    next.turn_order.iter().position(|p| p.character_id == source),
                                                    next.turn_order.iter().position(|p| p.character_id == drop_id)
                                                ) {
                                                    let moved = next.turn_order.remove(from);
                                                    next.turn_order.insert(to, moved);
                                                    next.current_index = active_id.and_then(|active| next.turn_order.iter().position(|p| p.character_id == active)).unwrap_or(0);
                                                    persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                                }
                                            }
                                        >
                                            <div class="turn-player-row">
                                                <span class="turn-drag-handle">"⠿"</span>
                                                <span class="turn-position">{index + 1}</span>
                                                <span class="turn-player-name">{participant.name.clone()}</span>
                                                {if is_current { view! { <small>"Em ação"</small> }.into_any() } else { view! { <span></span> }.into_any() }}
                                                <button type="button" class="turn-expand-button"
                                                    aria-label="Abrir observações e condição"
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        if expanded_turn_player.get_untracked() == expand_id {
                                                            expanded_turn_player.set(String::new());
                                                        } else {
                                                            expanded_turn_player.set(expand_id.clone());
                                                        }
                                                    }>
                                                    {move || if expanded_turn_player.get() == expand_label_id { "⌄" } else { "›" }}
                                                </button>
                                            </div>
                                            <Show when=move || expanded_turn_player.get() == show_details_id>
                                                <div class="turn-player-details">
                                                    <label>
                                                        <span>"Observações"</span>
                                                        <textarea rows="3" placeholder="Anotações para este jogador..."
                                                            prop:value=participant.notes.clone()
                                                            on:change=move |ev| {
                                                                let value = event_target_value(&ev);
                                                                if let Some(mut next) = combat_state.get_untracked() {
                                                                    for target in &mut next.turn_order {
                                                                        if target.character_id == notes_id {
                                                                            target.notes = value.clone();
                                                                            break;
                                                                        }
                                                                    }
                                                                    persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                                                }
                                                            }
                                                        ></textarea>
                                                    </label>
                                                    <label>
                                                        <span>"Condição"</span>
                                                        <select
                                                            prop:value=participant.condition_name.clone()
                                                            on:change={
                                                                let condition_id = condition_id.clone();
                                                                move |ev| {
                                                                let selected = event_target_value(&ev);
                                                                let description = conditions_library.get().into_iter()
                                                                    .find(|item| item.name == selected)
                                                                    .map(|item| item.description).unwrap_or_default();
                                                                if let Some(mut next) = combat_state.get_untracked() {
                                                                    for target in &mut next.turn_order {
                                                                        if target.character_id == condition_id {
                                                                            target.condition_name = selected.clone();
                                                                            target.condition_description = description.clone();
                                                                            break;
                                                                        }
                                                                    }
                                                                    persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                                                }
                                                            }
                                                        >
                                                            <option value="">"Sem condição"</option>
                                                            {move || conditions_library.get().into_iter().map(|condition| {
                                                                let name = condition.name.clone();
                                                                view! { <option value=name.clone()>{name}</option> }
                                                            }).collect_view()}
                                                        </select>
                                                    </label>
                                                    {if participant.condition_name.is_empty() {
                                                        view! { <p class="turn-detail-hint">"Nenhuma condição atribuída."</p> }.into_any()
                                                    } else {
                                                        view! { <p class="turn-condition-preview"><strong>{participant.condition_name.clone()}</strong>{participant.condition_description.clone()}</p> }.into_any()
                                                    }}
                                                </div>
                                            </Show>
                                        </li>
                                    }
                                }).collect_view()).unwrap_or_default()}
                            </ol>
                            <div class="combat-actions">
                                <button type="button" class="resource-edit" on:click=move |_| {
                                    if let Some(mut next) = combat_state.get_untracked() {
                                        if !next.turn_order.is_empty() {
                                            next.current_index += 1;
                                            if next.current_index >= next.turn_order.len() {
                                                next.current_index = 0;
                                                next.round += 1;
                                            }
                                            persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                        }
                                    }
                                }>"Avançar turno"</button>
                                <button type="button" class="danger" on:click=move |_| {
                                    if let Some(mut next) = combat_state.get_untracked() {
                                        next.is_active = false;
                                        next.participant_user_ids.clear();
                                        next.turn_order.clear();
                                        next.current_index = 0;
                                        next.round = 1;
                                        persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                    }
                                    combat_setup_open.set(false);
                                }>"Encerrar combate"</button>
                            </div>
                        </Show>
                    </section>
                </Show>

                <Show when=move || tab.get() == 0 && !master_mode.get()>
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
                                    disabled=move || portrait_uploading.get() || viewing_as_master.get()
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

                    <h2>"Recursos"</h2>
                    <div class="resource-grid">
                        <div class="resource-card resource-pv">
                            <strong>"PV"</strong>
                            <div class="resource-control" class:resource-low=move || { let s = sheet.get(); let max = if s.pv_max > 0 { s.pv_max } else { s.pv.max(1) }; s.pv <= max / 4 && s.pv <= max } class:resource-overflow=move || { let s = sheet.get(); let max = if s.pv_max > 0 { s.pv_max } else { s.pv.max(1) }; s.pv > max }>
                                <button type="button" aria-label="Diminuir vida em 5" on:click=move |_| sheet.update(|s| s.pv = (s.pv - 5).max(0))>"≪"</button>
                                <button type="button" aria-label="Diminuir vida em 1" on:click=move |_| sheet.update(|s| s.pv = (s.pv - 1).max(0))>"‹"</button>
                                <div class="resource-meter">
                                    <div class="resource-meter-fill" style:width=move || { let s = sheet.get(); let max = if s.pv_max > 0 { s.pv_max } else { s.pv.max(1) }; format!("{}%", (s.pv.max(0).min(max) as f64 / max as f64 * 100.0)) }></div>
                                    <span>{move || sheet.get().pv.to_string()}</span>
                                    <span class="resource-value-separator">"/"</span>
                                    <span class:resource-max-hidden=move || resource_editing.get()>{move || { let s = sheet.get(); if s.pv_max > 0 { s.pv_max } else { s.pv } }}</span>
                                    
                                    <input class="resource-inline-max" type="number" min="0" aria-label="Máximo de vida" class:resource-inline-max-visible=move || resource_editing.get() prop:value=move || { let s = sheet.get(); if s.pv_max > 0 { s.pv_max } else { s.pv }.to_string() } on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| { s.pv_max = v.max(0); }); } } />
                                </div>
                                <button type="button" aria-label="Aumentar vida em 1" on:click=move |_| sheet.update(|s| { let max = if s.pv_max > 0 { s.pv_max } else { i32::MAX }; s.pv = s.pv.saturating_add(1); })>"›"</button>
                                <button type="button" aria-label="Aumentar vida em 5" on:click=move |_| sheet.update(|s| { let max = if s.pv_max > 0 { s.pv_max } else { i32::MAX }; s.pv = s.pv.saturating_add(5); })>"≫"</button>
                            </div>
                        </div>
                        <div class="resource-card resource-san">
                            <strong>"SAN"</strong>
                            <div class="resource-control" class:resource-low=move || { let s = sheet.get(); let max = if s.san_max > 0 { s.san_max } else { s.san.max(1) }; s.san <= max / 4 && s.san <= max } class:resource-overflow=move || { let s = sheet.get(); let max = if s.san_max > 0 { s.san_max } else { s.san.max(1) }; s.san > max }>
                                <button type="button" aria-label="Diminuir sanidade em 5" on:click=move |_| sheet.update(|s| s.san = (s.san - 5).max(0))>"≪"</button>
                                <button type="button" aria-label="Diminuir sanidade em 1" on:click=move |_| sheet.update(|s| s.san = (s.san - 1).max(0))>"‹"</button>
                                <div class="resource-meter">
                                    <div class="resource-meter-fill" style:width=move || { let s = sheet.get(); let max = if s.san_max > 0 { s.san_max } else { s.san.max(1) }; format!("{}%", (s.san.max(0).min(max) as f64 / max as f64 * 100.0)) }></div>
                                    <span>{move || sheet.get().san.to_string()}</span>
                                    <span class="resource-value-separator">"/"</span>
                                    <span class:resource-max-hidden=move || resource_editing.get()>{move || { let s = sheet.get(); if s.san_max > 0 { s.san_max } else { s.san } }}</span>
                                    
                                    <input class="resource-inline-max" type="number" min="0" aria-label="Máximo de sanidade" class:resource-inline-max-visible=move || resource_editing.get() prop:value=move || { let s = sheet.get(); if s.san_max > 0 { s.san_max } else { s.san }.to_string() } on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| { s.san_max = v.max(0); }); } } />
                                </div>
                                <button type="button" aria-label="Aumentar sanidade em 1" on:click=move |_| sheet.update(|s| { let max = if s.san_max > 0 { s.san_max } else { i32::MAX }; s.san = s.san.saturating_add(1); })>"›"</button>
                                <button type="button" aria-label="Aumentar sanidade em 5" on:click=move |_| sheet.update(|s| { let max = if s.san_max > 0 { s.san_max } else { i32::MAX }; s.san = s.san.saturating_add(5); })>"≫"</button>
                            </div>
                        </div>
                        <div class="resource-card resource-pm">
                            <strong>"PM"</strong>
                            <div class="resource-control" class:resource-low=move || { let s = sheet.get(); let max = if s.pm_max > 0 { s.pm_max } else { s.pm.max(1) }; s.pm <= max / 4 && s.pm <= max } class:resource-overflow=move || { let s = sheet.get(); let max = if s.pm_max > 0 { s.pm_max } else { s.pm.max(1) }; s.pm > max }>
                                <button type="button" aria-label="Diminuir esforço em 5" on:click=move |_| sheet.update(|s| s.pm = (s.pm - 5).max(0))>"≪"</button>
                                <button type="button" aria-label="Diminuir esforço em 1" on:click=move |_| sheet.update(|s| s.pm = (s.pm - 1).max(0))>"‹"</button>
                                <div class="resource-meter">
                                    <div class="resource-meter-fill" style:width=move || { let s = sheet.get(); let max = if s.pm_max > 0 { s.pm_max } else { s.pm.max(1) }; format!("{}%", (s.pm.max(0).min(max) as f64 / max as f64 * 100.0)) }></div>
                                    <span>{move || sheet.get().pm.to_string()}</span>
                                    <span class="resource-value-separator">"/"</span>
                                    <span class:resource-max-hidden=move || resource_editing.get()>{move || { let s = sheet.get(); if s.pm_max > 0 { s.pm_max } else { s.pm } }}</span>
                                    
                                    <input class="resource-inline-max" type="number" min="0" aria-label="Máximo de esforço" class:resource-inline-max-visible=move || resource_editing.get() prop:value=move || { let s = sheet.get(); if s.pm_max > 0 { s.pm_max } else { s.pm }.to_string() } on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| { s.pm_max = v.max(0); }); } } />
                                </div>
                                <button type="button" aria-label="Aumentar esforço em 1" on:click=move |_| sheet.update(|s| { let max = if s.pm_max > 0 { s.pm_max } else { i32::MAX }; s.pm = s.pm.saturating_add(1); })>"›"</button>
                                <button type="button" aria-label="Aumentar esforço em 5" on:click=move |_| sheet.update(|s| { let max = if s.pm_max > 0 { s.pm_max } else { i32::MAX }; s.pm = s.pm.saturating_add(5); })>"≫"</button>
                            </div>
                        </div>
                    </div>

                    <h2>"Atributos"</h2>
                    <div class="stat-grid">
                        <div class="stat"><strong>"INT"</strong><input aria-label="Atributo INT" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[0].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[0] = v); } } /></div>
                        <div class="stat"><strong>"FIS"</strong><input aria-label="Atributo FIS" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[1].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[1] = v); } } /></div>
                        <div class="stat"><strong>"DOM"</strong><input aria-label="Atributo DOM" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[2].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[2] = v); } } /></div>
                        <div class="stat"><strong>"VEL"</strong><input aria-label="Atributo VEL" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[3].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[3] = v); } } /></div>
                        <div class="stat"><strong>"HAB"</strong><input aria-label="Atributo HAB" type="number" disabled=move || !resource_editing.get() prop:value=move || sheet.get().stats[4].to_string() on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { sheet.update(|s| s.stats[4] = v); } } /></div>
                    </div>

                    <section class="test-advantages-section">
                        <h2>"Vantagens"</h2>
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

                <Show when=move || tab.get() == 1 && !master_mode.get()>
                    <h2>"Habilidades"</h2>
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

                    <section class="conditions-section">
                        <div class="conditions-heading">
                            <h2>"Condições"</h2>
                            <button type="button" class="resource-edit" on:click=move |_| condition_form_open.update(|open| *open = !*open)>
                                {move || if condition_form_open.get() { "Cancelar" } else { "Adicionar condição" }}
                            </button>
                        </div>
                        <Show when=move || condition_form_open.get()>
                            <div class="condition-create-form">
                                <label>"Nome"
                                    <input placeholder="Nome da condição" prop:value=move || condition_name.get()
                                        on:input=move |ev| condition_name.set(event_target_value(&ev)) />
                                </label>
                                <label>"Descrição"
                                    <textarea rows="3" placeholder="Descreva o efeito da condição..."
                                        prop:value=move || condition_description.get()
                                        on:input=move |ev| condition_description.set(event_target_value(&ev))></textarea>
                                </label>
                                <button type="button" class="skill-add" on:click=add_condition>"Salvar condição"</button>
                            </div>
                        </Show>
                        <div class="conditions-list" aria-label="Glossário de condições">
                            {move || {
                                let items = if sheet.get().conditions.is_empty() { default_conditions() } else { sheet.get().conditions };
                                items.into_iter().map(|condition| view! {
                                    <details class="condition-entry">
                                        <summary>{condition.name}</summary>
                                        <p>{condition.description}</p>
                                    </details>
                                }).collect_view()
                            }}
                        </div>
                    </section>

                </Show>

                <Show when=move || tab.get() == 2 && !master_mode.get()>
                    <div class="notes-panel">
                        <h2>"Anotações"</h2>
                        <p class="hint">"Escreva.."</p>
                        <textarea class="notes" rows="23" placeholder="Escreva aqui..." prop:value=move || sheet.get().notes.clone() on:input=move |ev| sheet.update(|s| s.notes = event_target_value(&ev))></textarea>
                    </div>
                    <inventory::Inventory sheet=sheet />
                </Show>
            </section>

            <Show when=move || turn_popup_open.get() && !master_mode.get()>
                <div class="turn-notification-backdrop" role="dialog" aria-modal="true" aria-label="Aviso de turno">
                    <div class="turn-notification">
                        <span>"SAMSARA"</span>
                        <h2>"É a sua vez!"</h2>
                        <p>"Seu personagem está na vez de agir."</p>
                        {move || turn_popup_participant.get().map(|participant| view! {
                            <>
                                {if !participant.condition_name.is_empty() {
                                    view! {
                                        <section class="turn-popup-condition">
                                            <strong>{format!("Condição: {}", participant.condition_name)}</strong>
                                            <p>{participant.condition_description}</p>
                                        </section>
                                    }.into_any()
                                } else { view! { <></> }.into_any() }}
                                {if !participant.notes.trim().is_empty() {
                                    view! {
                                        <section class="turn-popup-notes">
                                            <strong>"Observações do Mestre"</strong>
                                            <p>{participant.notes}</p>
                                        </section>
                                    }.into_any()
                                } else { view! { <></> }.into_any() }}
                            </>
                        })}
                        <button type="button" on:click=move |_| turn_popup_open.set(false)>"Entendi"</button>
                    </div>
                </div>
            </Show>

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
                            restore_own_sheet();
                            master_mode.set(true);
                            tab.set(3);
                            load_master_roster();
                            save_status.set("Retornou ao painel do Mestre.".to_string());
                        }>"Voltar ao Mestre"</button>
                    </Show>
                    <Show when=move || !master_mode.get()>
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
                    </Show>

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
                            <img class="site-logo auth-logo" src="logo.png" alt="SAMSARA" />
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