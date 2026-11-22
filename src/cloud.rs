
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use js_sys::Uint8Array;

use crate::auth::AuthSession;

const SUPABASE_URL: &str = env!("SUPABASE_URL");
const SUPABASE_KEY: &str = env!("SUPABASE_ANON_KEY");
const PORTRAIT_BUCKET: &str = "portraits";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudCharacter {
    pub id: String,
    pub name: String,
    pub sheet_data: Value,
    pub portrait_path: Option<String>,
    #[serde(default)]
    pub user_id: String,
}



/// Participante de um combate compartilhado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombatParticipant {
    pub character_id: String,
    pub name: String,
    pub user_id: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub condition_name: String,
    #[serde(default)]
    pub condition_description: String,
}

/// Estado único do combate ativo da campanha.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatState {
    pub id: i32,
    pub is_active: bool,
    pub participant_user_ids: Vec<String>,
    pub turn_order: Vec<CombatParticipant>,
    pub current_index: usize,
    pub round: u32,
}

pub async fn load_combat_state(session: &AuthSession) -> Result<Option<CombatState>, String> {
    let url = format!(
        "{}/rest/v1/samsara_combat_state?select=id,is_active,participant_user_ids,turn_order,current_index,round&id=eq.1",
        SUPABASE_URL
    );
    let response = Request::get(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .send().await.map_err(|e| e.to_string())?;
    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao consultar combate ({}): {}", status, body));
    }
    let rows: Vec<CombatState> = response.json().await.map_err(|e| e.to_string())?;
    Ok(rows.into_iter().next())
}

pub async fn save_combat_state(session: &AuthSession, state: &CombatState) -> Result<(), String> {
    if session.user.id != MASTER_UID {
        return Err("Apenas a conta Mestre pode controlar os turnos.".into());
    }
    let body = serde_json::json!({
        "id": 1,
        "is_active": state.is_active,
        "participant_user_ids": state.participant_user_ids,
        "turn_order": state.turn_order,
        "current_index": state.current_index,
        "round": state.round,
    });
    let url = format!("{}/rest/v1/samsara_combat_state?on_conflict=id", SUPABASE_URL);
    let response = Request::post(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "resolution=merge-duplicates,return=minimal")
        .json(&body).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?;
    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao salvar combate ({}): {}", status, body));
    }
    Ok(())
}

pub const MASTER_UID: &str = "ac5dd4c6-6aa7-47a6-8192-703b44ef3961";

pub async fn list_all_characters(
    session: &AuthSession,
) -> Result<Vec<CloudCharacter>, String> {
    if session.user.id != MASTER_UID {
        return Err("Acesso restrito à conta Mestre.".into());
    }

    let url = format!(
        "{}/rest/v1/characters?select=id,name,sheet_data,portrait_path,user_id&order=created_at.desc",
        SUPABASE_URL
    );
    let response = Request::get(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao listar fichas ({}): {}", status, body));
    }
    response.json().await.map_err(|e| e.to_string())
}

pub async fn update_character_as_master(
    session: &AuthSession,
    character_id: &str,
    name: &str,
    portrait_path: Option<&str>,
    sheet_data: Value,
) -> Result<(), String> {
    if session.user.id != MASTER_UID {
        return Err("Acesso restrito à conta Mestre.".into());
    }
    let body = serde_json::json!({
        "name": name,
        "sheet_data": sheet_data,
        "portrait_path": portrait_path
    });
    let url = format!("{}/rest/v1/characters?id=eq.{}", SUPABASE_URL, character_id);
    let response = Request::patch(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(&body).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?;
    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao atualizar ficha como Mestre ({}): {}", status, body));
    }
    Ok(())
}

pub async fn delete_character_as_master(
    session: &AuthSession,
    character: &CloudCharacter,
) -> Result<(), String> {
    if session.user.id != MASTER_UID {
        return Err("Acesso restrito à conta Mestre.".into());
    }

    if let Some(path) = character.portrait_path.as_deref() {
        let url = format!("{}/storage/v1/object/{}/{}", SUPABASE_URL, PORTRAIT_BUCKET, path);
        let response = Request::delete(&url)
            .header("apikey", SUPABASE_KEY)
            .header("Authorization", &format!("Bearer {}", session.access_token))
            .send().await.map_err(|e| e.to_string())?;
        if !response.ok() && response.status() != 404 {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Erro ao excluir retrato ({}): {}", status, body));
        }
    }

    let url = format!("{}/rest/v1/characters?id=eq.{}", SUPABASE_URL, character.id);
    let response = Request::delete(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .send().await.map_err(|e| e.to_string())?;
    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao excluir ficha: {}", body));
    }
    Ok(())
}

pub async fn load_character(
    session: &AuthSession,
) -> Result<Option<CloudCharacter>, String> {
    let url = format!(
        "{}/rest/v1/characters?select=id,name,sheet_data,portrait_path&user_id=eq.{}&campaign_id=is.null&order=created_at.asc&limit=1",
        SUPABASE_URL,
        session.user.id
    );

    let response = Request::get(&url)
        .header("apikey", SUPABASE_KEY)
        .header(
            "Authorization",
            &format!("Bearer {}", session.access_token),
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Erro ao carregar ficha ({}): {}",
            status, body
        ));
    }

    let characters: Vec<CloudCharacter> = response
        .json()
        .await
        .map_err(|e| e.to_string())?;

    Ok(characters.into_iter().next())
}

pub async fn create_character(
    session: &AuthSession,
    name: &str,
    sheet_data: Value,
) -> Result<CloudCharacter, String> {
    let body = serde_json::json!({
        "user_id": session.user.id,
        "campaign_id": null,
        "name": name,
        "sheet_data": sheet_data,
        "portrait_path": null
    });

    let response = Request::post(
        &format!("{}/rest/v1/characters", SUPABASE_URL)
    )
        .header("apikey", SUPABASE_KEY)
        .header(
            "Authorization",
            &format!("Bearer {}", session.access_token),
        )
        .header("Content-Type", "application/json")
        .header("Prefer", "return=representation")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Erro ao criar ficha ({}): {}",
            status, body
        ));
    }

    let mut characters: Vec<CloudCharacter> = response
        .json()
        .await
        .map_err(|e| e.to_string())?;

    characters
        .pop()
        .ok_or_else(|| "O Supabase não retornou a ficha criada.".to_string())
}

pub async fn update_character(
    session: &AuthSession,
    character_id: &str,
    name: &str,
    portrait_path: Option<&str>,
    sheet_data: Value,
) -> Result<(), String> {
    let body = serde_json::json!({
        "name": name,
        "sheet_data": sheet_data,
        "portrait_path": portrait_path
    });

    let url = format!(
        "{}/rest/v1/characters?id=eq.{}&user_id=eq.{}",
        SUPABASE_URL,
        character_id,
        session.user.id
    );

    let response = Request::patch(&url)
        .header("apikey", SUPABASE_KEY)
        .header(
            "Authorization",
            &format!("Bearer {}", session.access_token),
        )
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Erro ao atualizar ficha ({}): {}",
            status, body
        ));
    }

    Ok(())
}


pub async fn upload_portrait(
    session: &AuthSession,
    character_id: &str,
    bytes: &[u8],
    content_type: &str,
    extension: &str,
) -> Result<(String, String), String> {
    let path = format!("{}/{}/portrait.{}", character_id, session.user.id, extension);
    let url = format!("{}/storage/v1/object/{}/{}", SUPABASE_URL, PORTRAIT_BUCKET, path);
    let body: wasm_bindgen::JsValue = Uint8Array::from(bytes).into();

    let response = Request::post(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .header("Content-Type", content_type)
        .header("x-upsert", "true")
        .body(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro no Storage ({}): {}", status, body));
    }

    let signed_url = portrait_signed_url(session, &path).await?;
    Ok((path, signed_url))
}

pub async fn portrait_signed_url(
    session: &AuthSession,
    path: &str,
) -> Result<String, String> {
    let url = format!("{}/storage/v1/object/sign/{}/{}", SUPABASE_URL, PORTRAIT_BUCKET, path);
    let body = serde_json::json!({ "expiresIn": 604800 });

    let response = Request::post(&url)
        .header("apikey", SUPABASE_KEY)
        .header("Authorization", &format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Erro ao gerar link do retrato ({}): {}", status, body));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let signed = result
        .get("signedURL")
        .or_else(|| result.get("signedUrl"))
        .and_then(Value::as_str)
        .ok_or("O Supabase não retornou o link do retrato.")?;

    if signed.starts_with("http://") || signed.starts_with("https://") {
        Ok(signed.to_string())
    } else if signed.starts_with("/storage/v1/") {
        Ok(format!("{}{}", SUPABASE_URL, signed))
    } else if signed.starts_with('/') {
        Ok(format!("{}/storage/v1{}", SUPABASE_URL, signed))
    } else {
        Ok(format!("{}/storage/v1/{}", SUPABASE_URL, signed))
    }
}
