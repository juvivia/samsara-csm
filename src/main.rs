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
    #[serde(default = "default_temperature")]
    temperature: i32,
    #[serde(default = "default_survival_state")]
    hunger_state: String,
    #[serde(default = "default_survival_state")]
    thirst_state: String,
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
            temperature: default_temperature(),
            hunger_state: default_survival_state(),
            thirst_state: default_survival_state(),
        }
    }
}

fn default_temperature() -> i32 { 20 }
fn effective_temperature(sheet: &Sheet) -> i32 {
    sheet.temperature - (sheet.stats[1] * 3) + (sheet.stats[2] * 3)
}
fn default_survival_state() -> String { "Normal".into() }


fn handle_portrait_change(
    ev: web_sys::Event,
    crop_source: RwSignal<Option<String>>,
    crop_zoom: RwSignal<f64>,
    crop_x: RwSignal<f64>,
    crop_y: RwSignal<f64>,
    save_status: RwSignal<String>,
) {
    let input = ev.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok());
    let file = input.and_then(|input| input.files().and_then(|files| files.get(0)));
    let Some(file) = file else { return; };

    let mime = file.type_();
    if !["image/jpeg", "image/png", "image/webp"].contains(&mime.as_str()) {
        save_status.set("Formato inválido. Use JPG, PNG ou WEBP.".to_string());
        return;
    }
    if file.size() > 5.0 * 1024.0 * 1024.0 {
        save_status.set("A imagem deve ter no máximo 5 MB.".to_string());
        return;
    }

    match web_sys::Url::create_object_url_with_blob(&file) {
        Ok(url) => {
            crop_source.set(Some(url));
            crop_zoom.set(1.0);
            crop_x.set(0.0);
            crop_y.set(0.0);
            save_status.set("Ajuste o enquadramento e confirme o recorte.".to_string());
        }
        Err(_) => save_status.set("Não foi possível abrir a imagem selecionada.".to_string()),
    }
}

fn confirm_portrait_crop(
    source_url: String,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
    session: auth::AuthSession,
    character_id: String,
    sheet: RwSignal<Sheet>,
    save_status: RwSignal<String>,
    portrait_url: RwSignal<Option<String>>,
    portrait_uploading: RwSignal<bool>,
    crop_source: RwSignal<Option<String>>,
) {
    let image = match web_sys::HtmlImageElement::new() {
        Ok(image) => image,
        Err(_) => {
            save_status.set("Não foi possível preparar o recorte.".to_string());
            return;
        }
    };

    let image_for_load = image.clone();
    let source_for_load = source_url.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        let result = (|| -> Result<Vec<u8>, String> {
            let canvas = web_sys::window()
                .ok_or("Navegador indisponível.".to_string())?
                .document()
                .ok_or("Documento indisponível.".to_string())?
                .create_element("canvas")
                .map_err(|_| "Não foi possível criar a imagem recortada.".to_string())?
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .map_err(|_| "Canvas indisponível.".to_string())?;

            // Retrato vertical 7:10 em alta resolução.
            let out_w = 700.0;
            let out_h = 1000.0;
            canvas.set_width(out_w as u32);
            canvas.set_height(out_h as u32);
            let ctx = canvas.get_context("2d")
                .map_err(|_| "Não foi possível preparar o recorte.".to_string())?
                .ok_or("Canvas 2D indisponível.".to_string())?
                .dyn_into::<web_sys::CanvasRenderingContext2d>()
                .map_err(|_| "Canvas 2D indisponível.".to_string())?;

            let iw = image_for_load.natural_width() as f64;
            let ih = image_for_load.natural_height() as f64;
            if iw <= 0.0 || ih <= 0.0 {
                return Err("A imagem selecionada é inválida.".to_string());
            }
            let scale = (out_w / iw).max(out_h / ih) * zoom;
            let draw_w = iw * scale;
            let draw_h = ih * scale;
            let x = (out_w - draw_w) * (0.5 + offset_x / 200.0);
            let y = (out_h - draw_h) * (0.5 + offset_y / 200.0);
            ctx.draw_image_with_html_image_element_and_dw_and_dh(
                &image_for_load, x, y, draw_w, draw_h
            ).map_err(|_| "Não foi possível recortar a imagem.".to_string())?;

            let data_url = canvas.to_data_url_with_type("image/png")
                .map_err(|_| "Não foi possível gerar a imagem recortada.".to_string())?;
            let encoded = data_url.split_once(',')
                .map(|(_, data)| data)
                .ok_or("Formato de imagem recortada inválido.".to_string())?;
            let binary = web_sys::window()
                .ok_or("Navegador indisponível.".to_string())?
                .atob(encoded)
                .map_err(|_| "Não foi possível converter o recorte.".to_string())?;
            Ok(binary.bytes().collect())
        })();

        match result {
            Ok(bytes) => {
                let session = session.clone();
                let character_id = character_id.clone();
                let sheet = sheet.clone();
                let save_status = save_status.clone();
                let portrait_url = portrait_url.clone();
                let portrait_uploading = portrait_uploading.clone();
                let crop_source = crop_source.clone();
                spawn_local(async move {
                    match cloud::upload_portrait(
                        &session, &character_id, &bytes, "image/png", "png"
                    ).await {
                        Ok((path, url)) => {
                            sheet.update(|s| s.portrait_path = Some(path));
                            portrait_url.set(Some(url));
                            save_status.set("Retrato recortado e enviado. Clique em Salvar ficha para confirmar.".to_string());
                            crop_source.set(None);
                        }
                        Err(error) => save_status.set(format!("Erro ao enviar imagem: {}", error)),
                    }
                    portrait_uploading.set(false);
                });
            }
            Err(error) => {
                save_status.set(error);
                portrait_uploading.set(false);
            }
        }
    });
    image.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    portrait_uploading.set(true);
    save_status.set("Preparando recorte...".to_string());
    image.set_src(&source_for_load);
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
        ConditionEntry { name: "Fadiga (sede)".into(), description: "Quando estiver com \"sede\" terá -1 de VEL, e se chegar a \"Desidratado\" aumenta para -2 de VEL.".into() },
        ConditionEntry { name: "Fadiga (fome)".into(), description: "Quando estiver com \"fome\" terá -1 de FIS e se chegar a \"Desnutrição\" aumenta para -2 de FIS.".into() },
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
    let crop_source = RwSignal::new(None::<String>);
    let crop_zoom = RwSignal::new(1.0f64);
    let crop_x = RwSignal::new(0.0f64);
    let crop_y = RwSignal::new(0.0f64);
    let selected_theme = RwSignal::new(load_color_theme(&character_id.get()));
    let theme_picker_open = RwSignal::new(false);
    let is_master = session.get_untracked().user.id == cloud::MASTER_UID;
    let viewing_as_master = RwSignal::new(false);
    let master_characters = RwSignal::new(Vec::<(cloud::CloudCharacter, Option<String>)>::new());
    let master_temperature = RwSignal::new(20i32);
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
    let master_info_notes = RwSignal::new(String::new());
    let combat_cards = RwSignal::new(Vec::<cloud::CombatSheetCard>::new());
    let combat_card_name = RwSignal::new(String::new());
    let combat_card_pv = RwSignal::new(10i32);
    let combat_card_pv_max = RwSignal::new(10i32);
    let combat_card_pm = RwSignal::new(5i32);
    let combat_card_pm_max = RwSignal::new(5i32);
    let combat_card_stats = RwSignal::new([0i32; 5]);
    let combat_card_form_open = RwSignal::new(false);
    let combat_cards_loading = RwSignal::new(false);
    let combat_cards_error = RwSignal::new(String::new());
    let combat_cards_saving = RwSignal::new(false);
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

    let load_combat_cards = move || {
        combat_cards_loading.set(true);
        combat_cards_error.set(String::new());
        let current_session = session.get();
        spawn_local(async move {
            match cloud::list_combat_cards(&current_session).await {
                Ok(cards) => combat_cards.set(cards),
                Err(error) => combat_cards_error.set(error),
            }
            combat_cards_loading.set(false);
        });
    };
    let load_master_notes = move || {
        let current_session = session.get();
        spawn_local(async move {
            match cloud::load_master_notes(&current_session).await {
                Ok(notes) => {
                    if notes.is_empty() {
                        let fallback = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
                            .and_then(|storage| storage.get_item("samsara-master-info-notes").ok().flatten())
                            .unwrap_or_default();
                        master_info_notes.set(fallback);
                    } else { master_info_notes.set(notes); }
                },
                Err(error) => combat_cards_error.set(error),
            }
        });
    };
    let save_master_workspace = move || {
        combat_cards_saving.set(true);
        combat_cards_error.set(String::new());
        let current_session = session.get();
        let notes = master_info_notes.get();
        let cards = combat_cards.get();
        spawn_local(async move {
            let result = async {
                cloud::save_master_notes(&current_session, &notes).await?;
                let mut saved_cards = Vec::new();
                for card in cards {
                    if card.id.is_empty() {
                        saved_cards.push(cloud::create_combat_card(&current_session, &card).await?);
                    } else {
                        cloud::update_combat_card(&current_session, &card).await?;
                        saved_cards.push(card);
                    }
                }
                combat_cards.set(saved_cards);
                Ok::<(), String>(())
            }.await;
            match result {
                Ok(()) => save_status.set("Informações e fichas de combate salvas no Supabase.".to_string()),
                Err(error) => combat_cards_error.set(error),
            }
            combat_cards_saving.set(false);
        });
    };

    let load_master_roster = move || {
        master_loading.set(true);
        master_error.set(String::new());
        let current_session = session.get();
        spawn_local(async move {
            match cloud::list_all_characters(&current_session).await {
                Ok(characters) => {
                    let mut cards = Vec::new();
                    for mut character in characters {
                        // Limpeza pontual: remove a condição antiga "cagada" da ficha Aniez.
                        let mut removed_old_condition = false;
                        if character.name.eq_ignore_ascii_case("aniez") {
                            if let Some(conditions) = character.sheet_data.get_mut("conditions").and_then(serde_json::Value::as_array_mut) {
                                let original_len = conditions.len();
                                conditions.retain(|condition| {
                                    !condition.get("name")
                                        .and_then(serde_json::Value::as_str)
                                        .map(|name| name.eq_ignore_ascii_case("cagada"))
                                        .unwrap_or(false)
                                });
                                removed_old_condition = conditions.len() != original_len;
                            }
                        }
                        if removed_old_condition {
                            if let Err(error) = cloud::update_character_as_master(
                                &current_session,
                                &character.id,
                                &character.name,
                                character.portrait_path.as_deref(),
                                character.sheet_data.clone(),
                            ).await {
                                master_error.set(format!("A ficha Aniez foi carregada, mas não foi possível remover a condição antiga: {}", error));
                            }
                        }
                        let portrait = match character.portrait_path.as_deref() {
                            Some(path) => cloud::portrait_signed_url(&current_session, path).await.ok(),
                            None => None,
                        };
                        cards.push((character, portrait));
                    }
                    let mut library = default_conditions();
                    for (character, _) in &cards {
                        let parsed = character.sheet_data.get("conditions")
                            .and_then(|value| serde_json::from_value::<Vec<ConditionEntry>>(value.clone()).ok())
                            .filter(|items| !items.is_empty())
                            .unwrap_or_else(default_conditions);
                        for condition in parsed {
                            if condition.name.eq_ignore_ascii_case("cagada") { continue; }
                            if !library.iter().any(|existing| existing.name.eq_ignore_ascii_case(&condition.name)) {
                                library.push(condition);
                            }
                        }
                    }
                    library.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                    conditions_library.set(library);
                    if let Some((first, _)) = cards.first() {
                        master_temperature.set(first.sheet_data.get("temperature")
                            .and_then(serde_json::Value::as_i64).unwrap_or(20) as i32);
                    }
                    master_characters.set(cards);
                }
                Err(error) => master_error.set(error),
            }
            master_loading.set(false);
        });
    };

    let update_roster_survival = move |id: String, field: &'static str, value: String| {
        let Some((mut target, _)) = master_characters.get_untracked().into_iter()
            .find(|(character, _)| character.id == id) else { return; };
        target.sheet_data[field] = serde_json::Value::String(value);
        let auth_session = session.get_untracked();
        let target_id = target.id.clone();
        let target_name = target.name.clone();
        let target_portrait = target.portrait_path.clone();
        let data = target.sheet_data.clone();
        spawn_local(async move {
            match cloud::update_character_as_master(
                &auth_session, &target_id, &target_name, target_portrait.as_deref(), data
            ).await {
                Ok(()) => {
                    master_characters.update(|items| {
                        if let Some(entry) = items.iter_mut().find(|(item, _)| item.id == target_id) {
                            entry.0 = target;
                        }
                    });
                    master_error.set(String::new());
                }
                Err(error) => master_error.set(format!("Não foi possível salvar o estado de sobrevivência: {}", error)),
            }
        });
    };

    let save_roster_temperature = move |value: i32| {
        master_temperature.set(value);
        let targets = master_characters.get_untracked().into_iter().map(|(mut character, portrait)| {
            if let Some(object) = character.sheet_data.as_object_mut() {
                object.insert("temperature".into(), serde_json::json!(value));
            }
            (character, portrait)
        }).collect::<Vec<_>>();
        master_characters.set(targets.clone());
        let auth_session = session.get_untracked();
        spawn_local(async move {
            for (character, _) in targets {
                if let Err(error) = cloud::update_character_as_master(
                    &auth_session,
                    &character.id,
                    &character.name,
                    character.portrait_path.as_deref(),
                    character.sheet_data.clone(),
                ).await {
                    master_error.set(format!("Não foi possível atualizar a temperatura de todas as fichas: {}", error));
                    return;
                }
            }
            master_error.set(String::new());
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
                        let mut library = default_conditions();
                        for character in characters {
                            let parsed = character.sheet_data.get("conditions")
                                .and_then(|value| serde_json::from_value::<Vec<ConditionEntry>>(value.clone()).ok())
                                .filter(|items| !items.is_empty())
                                .unwrap_or_else(default_conditions);
                            for condition in parsed {
                                if condition.name.eq_ignore_ascii_case("cagada") { continue; }
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
                    <button class:active=move || tab.get() == 5 on:click=move |_| { tab.set(5); load_master_notes(); }>"Info"</button>
                    <button class:active=move || tab.get() == 6 on:click=move |_| { tab.set(6); load_combat_cards(); }>"Combate"</button>
                </Show>
            </nav>

            <section class="content">
                <Show when=move || tab.get() == 3 && master_mode.get() && is_master>
                    <section class="master-dashboard">
                        <h2>"Painel do Mestre"</h2>
                        <section class="master-global-temperature">
                            <div class="master-global-temperature-heading">
                                <label for="master-temperature-slider">"Temperatura do ambiente"</label>
                                <strong>{move || format!("{} °C", master_temperature.get())}</strong>
                            </div>
                            <input
                                id="master-temperature-slider"
                                class="survival-temperature-slider"
                                type="range"
                                min="-30"
                                max="50"
                                step="1"
                                aria-label="Temperatura geral em graus Celsius"
                                prop:value=move || master_temperature.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                                        master_temperature.set(value.clamp(-30, 50));
                                    }
                                }
                                on:change=move |ev| {
                                    if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                                        save_roster_temperature(value.clamp(-30, 50));
                                    }
                                }
                            />
                            <div class="survival-range-labels"><span>"-30 °C"</span><span>"50 °C"</span></div>
                            <p>"A temperatura é aplicada a todas as fichas da campanha."</p>
                        </section>
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
                                let hunger_value = values.get("hunger_state").and_then(serde_json::Value::as_str).unwrap_or("Normal").to_string();
                                let thirst_value = values.get("thirst_state").and_then(serde_json::Value::as_str).unwrap_or("Normal").to_string();
                                let hunger_card_id = card_id.clone();
                                let thirst_card_id = card_id.clone();
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
                                            <div class="master-character-survival">
                                                <label>
                                                    "Fome"
                                                    <select
                                                        prop:value=hunger_value
                                                        on:change=move |ev| update_roster_survival(hunger_card_id.clone(), "hunger_state", event_target_value(&ev))
                                                    >
                                                        <option value="Cheio">"Cheio"</option>
                                                        <option value="Normal">"Normal"</option>
                                                        <option value="Fome">"Fome"</option>
                                                        <option value="Desnutrição">"Desnutrição"</option>
                                                    </select>
                                                </label>
                                                <label>
                                                    "Sede"
                                                    <select
                                                        prop:value=thirst_value
                                                        on:change=move |ev| update_roster_survival(thirst_card_id.clone(), "thirst_state", event_target_value(&ev))
                                                    >
                                                        <option value="Hidratado">"Hidratado"</option>
                                                        <option value="Normal">"Normal"</option>
                                                        <option value="Sede">"Sede"</option>
                                                        <option value="Desidratado">"Desidratado"</option>
                                                    </select>
                                                </label>
                                            </div>
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
                                                <button type="button" class="inventory-icon-button inventory-delete-icon master-delete-icon" aria-label="Confirmar exclusão da ficha" title="Confirmar exclusão" on:click=move |_| {
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
                                                }><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg></button>
                                            </Show>
                                            <button type="button" class="inventory-icon-button inventory-delete-icon master-delete-icon" aria-label="Excluir ficha" title="Excluir ficha" on:click=move |_| master_delete_pending.set(button_card_id.clone())><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg></button>
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
                                                    conditions: Vec::new(),
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
                                {move || combat_state.get().map(|state| { let current_index = state.current_index; state.turn_order.into_iter().enumerate().map(|(index, participant)| {
                                    let id = participant.character_id.clone();
                                    let drag_id = id.clone();
                                    let drop_id = id.clone();
                                    let expand_id = id.clone();
                                    let expand_label_id = id.clone();
                                    let show_details_id = id.clone();
                                    let notes_id = id.clone();
                                    let condition_id = id.clone();
                                    let assigned_conditions = if participant.conditions.is_empty() {
                                        if participant.condition_name.is_empty() { Vec::new() } else {
                                            vec![cloud::CombatCondition {
                                                name: participant.condition_name.clone(),
                                                description: participant.condition_description.clone(),
                                            }]
                                        }
                                    } else {
                                        participant.conditions.clone()
                                    };
                                    let is_current = index == current_index;
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
                                                            on:change={
                                                                let notes_id = notes_id.clone();
                                                                move |ev| {
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
                                                            }
                                                        ></textarea>
                                                    </label>
                                                    <label>
                                                        <span>"Adicionar condição"</span>
                                                        <select
                                                            on:change={
                                                                let condition_id = condition_id.clone();
                                                                move |ev| {
                                                                    let selected = event_target_value(&ev);
                                                                    if selected.is_empty() { return; }
                                                                    let Some(condition) = conditions_library.get().into_iter().find(|item| item.name == selected) else { return; };
                                                                    if let Some(mut next) = combat_state.get_untracked() {
                                                                        for target in &mut next.turn_order {
                                                                            if target.character_id == condition_id {
                                                                                if target.conditions.is_empty() && !target.condition_name.is_empty() {
                                                                                    target.conditions.push(cloud::CombatCondition {
                                                                                        name: target.condition_name.clone(),
                                                                                        description: target.condition_description.clone(),
                                                                                    });
                                                                                }
                                                                                if !target.conditions.iter().any(|item| item.name.eq_ignore_ascii_case(&condition.name)) {
                                                                                    target.conditions.push(cloud::CombatCondition {
                                                                                        name: condition.name.clone(),
                                                                                        description: condition.description.clone(),
                                                                                    });
                                                                                }
                                                                                target.condition_name.clear();
                                                                                target.condition_description.clear();
                                                                                break;
                                                                            }
                                                                        }
                                                                        persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                                                    }
                                                                }
                                                            }
                                                        >
                                                            <option value="">"Selecione para adicionar..."</option>
                                                            {move || conditions_library.get().into_iter().map(|condition| {
                                                                let name = condition.name.clone();
                                                                view! { <option value=name.clone()>{name.clone()}</option> }
                                                            }).collect_view()}
                                                        </select>
                                                    </label>
                                                    <div class="turn-assigned-conditions">
                                                        <strong>"Condições ativas"</strong>
                                                        {if assigned_conditions.is_empty() {
                                                            view! { <p class="turn-detail-hint">"Nenhuma condição atribuída."</p> }.into_any()
                                                        } else {
                                                            view! {
                                                                <div class="turn-condition-list">
                                                                    {assigned_conditions.clone().into_iter().map(|condition| {
                                                                        let remove_id = condition_id.clone();
                                                                        let remove_name = condition.name.clone();
                                                                        view! {
                                                                            <div class="turn-condition-item">
                                                                                <div class="turn-condition-preview">
                                                                                    <strong>{condition.name}</strong>
                                                                                    <span>{condition.description}</span>
                                                                                </div>
                                                                                <button type="button" class="turn-condition-remove"
                                                                                    on:click=move |_| {
                                                                                        if let Some(mut next) = combat_state.get_untracked() {
                                                                                            for target in &mut next.turn_order {
                                                                                                if target.character_id == remove_id {
                                                                                                    if target.conditions.is_empty() && !target.condition_name.is_empty() {
                                                                                                        target.conditions.push(cloud::CombatCondition {
                                                                                                            name: target.condition_name.clone(),
                                                                                                            description: target.condition_description.clone(),
                                                                                                        });
                                                                                                    }
                                                                                                    target.conditions.retain(|item| item.name != remove_name);
                                                                                                    target.condition_name.clear();
                                                                                                    target.condition_description.clear();
                                                                                                    break;
                                                                                                }
                                                                                            }
                                                                                            persist_combat_state(session.get(), next, combat_state.clone(), combat_error.clone());
                                                                                        }
                                                                                    }
                                                                                ><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg></button>
                                                                            </div>
                                                                        }
                                                                    }).collect_view()}
                                                                </div>
                                                            }.into_any()
                                                        }}
                                                    </div>
                                                </div>
                                            </Show>
                                        </li>
                                    }
                                }).collect_view() }).unwrap_or_default()}
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

                <Show when=move || tab.get() == 5 && master_mode.get() && is_master>
                    <section class="master-info-dashboard">
                        <h2>"Info"</h2>
                        <div class="master-info-notes">
                            <h3>"Anotações do Mestre"</h3>
                            <textarea rows="12" placeholder="Anotações gerais da campanha..." prop:value=move || master_info_notes.get()
                                on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    master_info_notes.set(value.clone());
                                }
                            ></textarea>
                        </div>
                        <section class="master-tests-reference">
                            <h3>"Tipos de testes"</h3>
                            <div class="master-tests-grid">
                                {[
                                    ("Desviar","VEL + HAB"),("Sobrevivência","FIS + INT / HAB"),("Social","DOM"),
                                    ("Combate","FIS + HAB / VEL"),("Escapar","VEL + FIS"),("Pontaria","HAB"),
                                    ("Inventar","HAB + INT"),("Medicina","DOM + INT"),("Investigação","INT"),
                                    ("Elusão","HAB"),("Capacidade","FIS"),("Acrobacia","VEL + HAB"),
                                    ("Iniciativa","VEL"),("Percepção","HAB + DOM"),("Resistir","FIS + DOM"),("Dharma","DOM")
                                ].into_iter().map(|(name, formula)| view! {
                                    <article class="master-test-reference-card"><strong>{name}</strong><span>{formula}</span></article>
                                }).collect_view()}
                            </div>
                        </section>
                    </section>
                </Show>

                <Show when=move || tab.get() == 6 && master_mode.get() && is_master>
                    <section class="master-combat-sheets">
                        <h2>"Combate"</h2>
                        <button type="button" class="resource-edit combat-add-button" on:click=move |_| {
                            combat_card_form_open.update(|open| *open = !*open);
                            combat_card_name.set(String::new()); combat_card_stats.set([0;5]);
                            combat_card_pv.set(10); combat_card_pv_max.set(10);
                            combat_card_pm.set(5); combat_card_pm_max.set(5);
                        }>{move || if combat_card_form_open.get() { "Cancelar" } else { "Adicionar" }}</button>
                        <Show when=move || combat_card_form_open.get()>
                            <form class="combat-card-editor" on:submit=move |ev| {
                                ev.prevent_default();
                                let name=combat_card_name.get().trim().to_string();
                                if name.is_empty() { combat_cards_error.set("Informe um nome para a ficha.".into()); return; }
                                combat_cards.update(|items| items.push(cloud::CombatSheetCard {
                                    id:String::new(), name, stats:combat_card_stats.get(),
                                    pv:combat_card_pv.get(), pv_max:combat_card_pv_max.get(),
                                    pm:combat_card_pm.get(), pm_max:combat_card_pm_max.get(),
                                }));
                                combat_card_form_open.set(false); combat_cards_error.set(String::new());
                            }>
                                <h3>"Nova ficha"</h3>
                                <label>"Nome"<input required placeholder="Nome da criatura ou personagem" prop:value=move || combat_card_name.get() on:input=move |ev| combat_card_name.set(event_target_value(&ev)) /></label>
                                <div class="combat-card-stats">
                                    {["INT","FIS","DOM","VEL","HAB"].into_iter().enumerate().map(|(index,label)| view! {
                                        <label>{label}<input type="number" prop:value=move || combat_card_stats.get()[index].to_string()
                                            on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_card_stats.update(|stats| stats[index]=v); } } /></label>
                                    }).collect_view()}
                                </div>
                                <div class="combat-card-form-resources">
                                    <div class="combat-resource-edit">
                                        <strong>"PV"</strong>
                                        <div class="resource-control">
                                            <button type="button" aria-label="Diminuir PV em 5" on:click=move |_| combat_card_pv.update(|v| *v=(*v-5).max(0))>"≪"</button>
                                            <button type="button" aria-label="Diminuir PV" on:click=move |_| combat_card_pv.update(|v| *v=(*v-1).max(0))>"‹"</button>
                                            <div class="resource-meter">
                                                <div class="resource-meter-fill" style:width=move || { let max=combat_card_pv_max.get(); let current=combat_card_pv.get(); format!("{}%",if max>0 { current.max(0).min(max) as f64/max as f64*100.0 } else { 0.0 }) }></div>
                                                <span>{move || combat_card_pv.get().to_string()}</span><span class="resource-value-separator">"/"</span>
                                                <input class="combat-resource-max" type="number" min="0" aria-label="PV máximo" prop:value=move || combat_card_pv_max.get().to_string()
                                                    on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_card_pv_max.set(v.max(0)); } } />
                                            </div>
                                            <button type="button" aria-label="Aumentar PV" on:click=move |_| combat_card_pv.update(|v| *v=v.saturating_add(1))>"›"</button>
                                            <button type="button" aria-label="Aumentar PV em 5" on:click=move |_| combat_card_pv.update(|v| *v=v.saturating_add(5))>"≫"</button>
                                        </div>
                                    </div>
                                    <div class="combat-resource-edit">
                                        <strong>"PM"</strong>
                                        <div class="resource-control">
                                            <button type="button" aria-label="Diminuir PM em 5" on:click=move |_| combat_card_pm.update(|v| *v=(*v-5).max(0))>"≪"</button>
                                            <button type="button" aria-label="Diminuir PM" on:click=move |_| combat_card_pm.update(|v| *v=(*v-1).max(0))>"‹"</button>
                                            <div class="resource-meter">
                                                <div class="resource-meter-fill" style:width=move || { let max=combat_card_pm_max.get(); let current=combat_card_pm.get(); format!("{}%",if max>0 { current.max(0).min(max) as f64/max as f64*100.0 } else { 0.0 }) }></div>
                                                <span>{move || combat_card_pm.get().to_string()}</span><span class="resource-value-separator">"/"</span>
                                                <input class="combat-resource-max" type="number" min="0" aria-label="PM máximo" prop:value=move || combat_card_pm_max.get().to_string()
                                                    on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_card_pm_max.set(v.max(0)); } } />
                                            </div>
                                            <button type="button" aria-label="Aumentar PM" on:click=move |_| combat_card_pm.update(|v| *v=v.saturating_add(1))>"›"</button>
                                            <button type="button" aria-label="Aumentar PM em 5" on:click=move |_| combat_card_pm.update(|v| *v=v.saturating_add(5))>"≫"</button>
                                        </div>
                                    </div>
                                </div>
                                <button type="submit">"Criar ficha"</button>
                            </form>
                        </Show>
                        <Show when=move || !combat_cards_error.get().is_empty()><p class="auth-error">{move || combat_cards_error.get()}</p></Show>
                        <Show when=move || combat_cards_loading.get()><p>"Carregando fichas..."</p></Show>
                        <div class="combat-cards-grid">
                            {move || combat_cards.get().into_iter().enumerate().map(|(idx,card)| {
                                let remove_id=card.id.clone();
                                view! {
                                    <article class="combat-mini-card">
                                        <input class="combat-card-title-input" aria-label="Nome da ficha" prop:value=card.name.clone()
                                            on:input=move |ev| { let value=event_target_value(&ev); combat_cards.update(|items| if let Some(item)=items.get_mut(idx) { item.name=value; }); } />
                                        <div class="combat-mini-stats">
                                            {["INT","FIS","DOM","VEL","HAB"].into_iter().enumerate().map(move |(stat_idx,label)| view! {
                                                <label><strong>{label}</strong><input type="number" aria-label=label
                                                    prop:value=move || combat_cards.get().get(idx).map(|c| c.stats[stat_idx].to_string()).unwrap_or_default()
                                                    on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.stats[stat_idx]=v; }); } } /></label>
                                            }).collect_view()}
                                        </div>
                                        <div class="combat-resource-edit">
                                            <strong>"PV"</strong>
                                            <div class="resource-control" class:resource-low=move || combat_cards.get().get(idx).map(|c| c.pv_max>0 && c.pv<=c.pv_max/4).unwrap_or(false)
                                                class:resource-overflow=move || combat_cards.get().get(idx).map(|c| c.pv_max>0 && c.pv>c.pv_max).unwrap_or(false)>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pv=(c.pv-5).max(0); })>"≪"</button>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pv=(c.pv-1).max(0); })>"‹"</button>
                                                <div class="resource-meter">
                                                    <div class="resource-meter-fill" style:width=move || combat_cards.get().get(idx).map(|c| format!("{}%",if c.pv_max>0 {c.pv.max(0).min(c.pv_max) as f64/c.pv_max as f64*100.0} else {0.0})).unwrap_or_default()></div>
                                                    <span>{move || combat_cards.get().get(idx).map(|c| c.pv.to_string()).unwrap_or_default()}</span><span class="resource-value-separator">"/"</span>
                                                    <input class="combat-resource-max" type="number" min="0" aria-label="PV máximo" prop:value=move || combat_cards.get().get(idx).map(|c| c.pv_max.to_string()).unwrap_or_default()
                                                        on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pv_max=v.max(0); }); } } />
                                                </div>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pv=c.pv.saturating_add(1); })>"›"</button>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pv=c.pv.saturating_add(5); })>"≫"</button>
                                            </div>
                                            <strong>"PM"</strong>
                                            <div class="resource-control" class:resource-low=move || combat_cards.get().get(idx).map(|c| c.pm_max>0 && c.pm<=c.pm_max/4).unwrap_or(false)
                                                class:resource-overflow=move || combat_cards.get().get(idx).map(|c| c.pm_max>0 && c.pm>c.pm_max).unwrap_or(false)>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pm=(c.pm-5).max(0); })>"≪"</button>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pm=(c.pm-1).max(0); })>"‹"</button>
                                                <div class="resource-meter">
                                                    <div class="resource-meter-fill" style:width=move || combat_cards.get().get(idx).map(|c| format!("{}%",if c.pm_max>0 {c.pm.max(0).min(c.pm_max) as f64/c.pm_max as f64*100.0} else {0.0})).unwrap_or_default()></div>
                                                    <span>{move || combat_cards.get().get(idx).map(|c| c.pm.to_string()).unwrap_or_default()}</span><span class="resource-value-separator">"/"</span>
                                                    <input class="combat-resource-max" type="number" min="0" aria-label="PM máximo" prop:value=move || combat_cards.get().get(idx).map(|c| c.pm_max.to_string()).unwrap_or_default()
                                                        on:input=move |ev| { if let Ok(v)=event_target_value(&ev).parse::<i32>() { combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pm_max=v.max(0); }); } } />
                                                </div>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pm=c.pm.saturating_add(1); })>"›"</button>
                                                <button type="button" on:click=move |_| combat_cards.update(|items| if let Some(c)=items.get_mut(idx) { c.pm=c.pm.saturating_add(5); })>"≫"</button>
                                            </div>
                                        </div>
                                        <button type="button" class="inventory-icon-button inventory-delete-icon master-delete-icon" aria-label="Excluir ficha de combate" title="Excluir ficha de combate" on:click=move |_| {
                                            let id=remove_id.clone();
                                            if id.is_empty() { combat_cards.update(|items| { if idx<items.len() { items.remove(idx); } }); }
                                            else {
                                                let current_session=session.get();
                                                spawn_local(async move {
                                                    match cloud::delete_combat_card(&current_session,&id).await {
                                                        Ok(())=>combat_cards.update(|items|items.retain(|item|item.id!=id)),
                                                        Err(error)=>combat_cards_error.set(error),
                                                    }
                                                });
                                            }
                                        }><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                    <path d="M3 6h18"/>
                                                    <path d="M8 6V4h8v2"/>
                                                    <path d="m19 6-1 14H6L5 6"/>
                                                    <path d="M10 11v5M14 11v5"/>
                                                </svg></button>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                        <Show when=move || !combat_cards_loading.get() && combat_cards.get().is_empty() && !combat_card_form_open.get()>
                            <p class="hint">"Nenhuma ficha de combate criada. Clique em Adicionar para começar."</p>
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
                                            crop_source.clone(),
                                            crop_zoom.clone(),
                                            crop_x.clone(),
                                            crop_y.clone(),
                                            save_status.clone(),
                                        );
                                    }
                                />
                            </label>
                        </div>
                        <Show when=move || crop_source.get().is_some()>
                            <div class="portrait-crop-backdrop">
                                <section class="portrait-crop-dialog" role="dialog" aria-modal="true" aria-label="Ajustar retrato">
                                    <h2>"Ajustar retrato"</h2>
                                    <p>"Arraste a imagem para escolher o enquadramento e ajuste o zoom."</p>
                                    <div
                                        class="portrait-crop-preview"
                                        on:pointermove=move |ev: web_sys::PointerEvent| {
                                            if ev.buttons() == 1 {
                                                crop_x.update(|v| *v = (*v + ev.movement_x() as f64 * 0.8).clamp(-100.0, 100.0));
                                                crop_y.update(|v| *v + ev.movement_y() as f64 * 0.8).clamp(-100.0, 100.0);
                                            }
                                        }
                                    >
                                        {move || crop_source.get().map(|src| view! {
                                            <img src=src alt="Prévia do recorte" draggable="false"
                                                style=move || format!(
                                                    "object-position: calc(50% + {}%) calc(50% + {}%); transform: scale({});",
                                                    crop_x.get(), crop_y.get(), crop_zoom.get()
                                                )
                                            />
                                        })}
                                    </div>
                                    <label class="portrait-zoom-control">
                                        "Zoom"
                                        <input type="range" min="1" max="3" step="0.05"
                                            prop:value=move || crop_zoom.get().to_string()
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { crop_zoom.set(v); }
                                            }
                                        />
                                    </label>
                                    <div class="portrait-crop-actions">
                                        <button class="secondary-button" type="button"
                                            disabled=move || portrait_uploading.get()
                                            on:click=move |_| {
                                                if let Some(url) = crop_source.get() {
                                                    let _ = web_sys::Url::revoke_object_url(&url);
                                                }
                                                crop_source.set(None);
                                            }
                                        >"Cancelar"</button>
                                        <button type="button" disabled=move || portrait_uploading.get()
                                            on:click=move |_| {
                                                if let Some(url) = crop_source.get() {
                                                    confirm_portrait_crop(
                                                        url, crop_zoom.get(), crop_x.get(), crop_y.get(),
                                                        session.get(), character_id.get(), sheet.clone(),
                                                        save_status.clone(), portrait_url.clone(),
                                                        portrait_uploading.clone(), crop_source.clone()
                                                    );
                                                }
                                            }
                                        >{move || if portrait_uploading.get() { "Processando..." } else { "Confirmar recorte" }}</button>
                                    </div>
                                </section>
                            </div>
                        </Show>
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

<section class="survival-section">
                        <Show
                            when=move || viewing_as_master.get()
                            fallback=move || view! {
                                <div class="survival-player-status">
                                    <div
                                        class=move || {
                                            let current_sheet = sheet.get();
                                            let temperature = effective_temperature(&current_sheet);
                                            if temperature < 10 {
                                                "survival-player-value survival-temperature-value survival-cold"
                                            } else if temperature > 35 {
                                                "survival-player-value survival-temperature-value survival-hot"
                                            } else {
                                                "survival-player-value survival-temperature-value"
                                            }
                                        }
                                    >
                                        <span class="survival-player-icon" aria-hidden="true">
                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M14 14.7V5a3 3 0 0 0-6 0v9.7a5 5 0 1 0 6 0Z"/>
                                                <path d="M11 11v7"/>
                                            </svg>
                                        </span>
                                        <span class="survival-player-label">"Temperatura"</span>
                                        <strong>{move || { let current_sheet = sheet.get(); format!("{} °C", effective_temperature(&current_sheet)) }}</strong>
                                    </div>
                                    <div class="survival-player-value">
                                        <span class="survival-player-icon" aria-hidden="true">
                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M7 3v7M10 3v7M13 3v7M10 10v11"/>
                                                <path d="M7 10h6"/>
                                                <path d="M17 3v18"/>
                                                <path d="M17 3c2 2 3 4 3 7h-3"/>
                                            </svg>
                                        </span>
                                        <span class="survival-player-label">"Fome"</span>
                                        <strong>{move || sheet.get().hunger_state.clone()}</strong>
                                    </div>
                                    <div class="survival-player-value">
                                        <span class="survival-player-icon" aria-hidden="true">
                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M12 3.2c-2.8 4-6 7.2-6 11a6 6 0 0 0 12 0c0-3.8-3.2-7-6-11Z"/>
                                            </svg>
                                        </span>
                                        <span class="survival-player-label">"Sede"</span>
                                        <strong>{move || sheet.get().thirst_state.clone()}</strong>
                                    </div>
                                </div>
                            }
                        >
                            <div class="survival-temperature">
                                <div class="survival-field-heading">
                                    <label>"Temperatura"</label>
                                    <strong>{move || format!("{} °C", sheet.get().temperature)}</strong>
                                </div>
                                <input
                                    class="survival-temperature-slider"
                                    type="range"
                                    min="-30"
                                    max="50"
                                    step="1"
                                    aria-label="Temperatura em graus Celsius"
                                    prop:value=move || sheet.get().temperature.to_string()
                                    on:input=move |ev| {
                                        if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                                            sheet.update(|s| s.temperature = value.clamp(-30, 50));
                                        }
                                    }
                                />
                                <div class="survival-range-labels"><span>"-30 °C"</span><span>"50 °C"</span></div>
                            </div>
                            <div class="survival-needs">
                                <label>
                                    "Fome"
                                    <select
                                        prop:value=move || sheet.get().hunger_state.clone()
                                        on:change=move |ev| sheet.update(|s| s.hunger_state = event_target_value(&ev))
                                    >
                                        <option value="Cheio">"Cheio"</option>
                                        <option value="Normal">"Normal"</option>
                                        <option value="Fome">"Fome"</option>
                                        <option value="Desnutrição">"Desnutrição"</option>
                                    </select>
                                </label>
                                <label>
                                    "Sede"
                                    <select
                                        prop:value=move || sheet.get().thirst_state.clone()
                                        on:change=move |ev| sheet.update(|s| s.thirst_state = event_target_value(&ev))
                                    >
                                        <option value="Hidratado">"Hidratado"</option>
                                        <option value="Normal">"Normal"</option>
                                        <option value="Sede">"Sede"</option>
                                        <option value="Desidratado">"Desidratado"</option>
                                    </select>
                                </label>
                            </div>
                            <p class="survival-master-hint">"Ajuste os valores e use “Salvar ficha” no rodapé para atualizar a ficha do jogador."</p>
                        </Show>
                    </section>

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
                        </div>
                        <div class="conditions-list" aria-label="Glossário de condições">
                            {move || {
                                let current_sheet = sheet.get();
                                // Mantém no painel do jogador a mesma base de condições do Mestre.
                                // As condições padrão sempre aparecem; as personalizadas da ficha são
                                // acrescentadas sem duplicar nomes, preservando suas descrições.
                                let mut items = default_conditions();
                                for condition in current_sheet.conditions.iter().cloned() {
                                    if !items.iter().any(|existing| existing.name.eq_ignore_ascii_case(&condition.name)) {
                                        items.push(condition);
                                    }
                                }
                                items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

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
                    <div class="notes-inventory-layout">
                        <div class="notes-panel">
                            <h2>"Anotações"</h2>
                            <p class="hint">"Escreva.."</p>
                            <textarea class="notes" rows="23" placeholder="Escreva aqui..." prop:value=move || sheet.get().notes.clone() on:input=move |ev| sheet.update(|s| s.notes = event_target_value(&ev))></textarea>
                        </div>
                        <inventory::Inventory sheet=sheet />
                    </div>
                </Show>
            </section>

            <Show when=move || turn_popup_open.get() && !master_mode.get()>
                <div class="turn-notification-backdrop" role="dialog" aria-modal="true" aria-label="Aviso de turno">
                    <div class="turn-notification">
                        <h2>"É a sua vez!"</h2>
                        <p>"Seu personagem está na vez de agir."</p>
                        {move || turn_popup_participant.get().map(|participant| view! {
                            <>
                                {let popup_conditions = if participant.conditions.is_empty() {
                                    if participant.condition_name.is_empty() { Vec::new() } else {
                                        vec![cloud::CombatCondition {
                                            name: participant.condition_name.clone(),
                                            description: participant.condition_description.clone(),
                                        }]
                                    }
                                } else { participant.conditions.clone() };
                                view! {
                                    {popup_conditions.into_iter().map(|condition| view! {
                                        <section class="turn-popup-condition">
                                            <strong>{format!("Condição: {}", condition.name)}</strong>
                                            <p>{condition.description}</p>
                                        </section>
                                    }).collect_view()}
                                }}
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

                    <Show when=move || master_mode.get() && is_master && (tab.get() == 5 || tab.get() == 6)>
                        <button type="button" class="resource-edit" disabled=move || combat_cards_saving.get()
                            on:click=move |_| save_master_workspace()>
                            {move || if combat_cards_saving.get() { "Salvando..." } else { "Salvar ficha" }}
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