// SPDX-License-Identifier: GPL-3.0-or-later
//! Traducción entre el formato de mensajes de Anthropic (el que usa el bucle
//! del asistente) y el de «chat completions» de OpenAI, que también aceptan
//! muchos otros servicios (Gemini, Mistral, Ollama, LM Studio…).
//!
//! El bucle de herramientas no cambia: el transporte convierte la petición a
//! la ida y la respuesta a la vuelta.

use serde_json::{Map, Value, json};

/// Petición de Anthropic → petición de chat completions.
///
/// `max_completion_tokens`: OpenAI lo pide así en sus modelos nuevos; los
/// servicios compatibles suelen esperar `max_tokens`.
pub fn peticion(cuerpo: &Value, max_completion_tokens: bool) -> Value {
    let mut mensajes = Vec::new();
    let sistema = texto_de(&cuerpo["system"]);
    if !sistema.is_empty() {
        mensajes.push(json!({ "role": "system", "content": sistema }));
    }
    for m in cuerpo["messages"].as_array().into_iter().flatten() {
        let rol = m["role"].as_str().unwrap_or("user");
        match &m["content"] {
            Value::String(t) => mensajes.push(json!({ "role": rol, "content": t })),
            Value::Array(bloques) if rol == "assistant" => mensajes.push(asistente(bloques)),
            Value::Array(bloques) => mensajes.extend(usuario(bloques)),
            _ => {}
        }
    }
    let herramientas: Vec<Value> = cuerpo["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            json!({
                "type": "function",
                "function": {
                    "name": h["name"],
                    "description": h["description"],
                    "parameters": h["input_schema"],
                }
            })
        })
        .collect();
    let mut p = Map::new();
    p.insert("model".into(), cuerpo["model"].clone());
    p.insert("messages".into(), Value::Array(mensajes));
    if !herramientas.is_empty() {
        p.insert("tools".into(), Value::Array(herramientas));
    }
    let clave = if max_completion_tokens {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    p.insert(clave.into(), cuerpo["max_tokens"].clone());
    Value::Object(p)
}

/// Respuesta de chat completions → respuesta de Anthropic (bloques `text` y
/// `tool_use`, `stop_reason` y `usage`).
pub fn respuesta(r: &Value) -> Value {
    let eleccion = &r["choices"][0];
    let m = &eleccion["message"];
    let mut bloques = Vec::new();
    if let Some(t) = m["content"].as_str().filter(|t| !t.trim().is_empty()) {
        bloques.push(json!({ "type": "text", "text": t }));
    }
    for (i, c) in m["tool_calls"].as_array().into_iter().flatten().enumerate() {
        let args = c["function"]["arguments"].as_str().unwrap_or("{}");
        let input: Value = serde_json::from_str(args).unwrap_or_else(|_| json!({}));
        let id = c["id"].as_str().map_or_else(|| format!("llamada_{i}"), str::to_owned);
        bloques.push(json!({
            "type": "tool_use",
            "id": id,
            "name": c["function"]["name"],
            "input": input,
        }));
    }
    let hay_herramientas = bloques.iter().any(|b| b["type"] == "tool_use");
    let stop = match eleccion["finish_reason"].as_str() {
        _ if hay_herramientas => "tool_use",
        Some("length") => "max_tokens",
        _ => "end_turn",
    };
    let u = &r["usage"];
    let entrada = u["prompt_tokens"].as_u64().unwrap_or(0);
    let cache = u["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
    json!({
        "content": bloques,
        "stop_reason": stop,
        "usage": {
            "input_tokens": entrada.saturating_sub(cache),
            "output_tokens": u["completion_tokens"].as_u64().unwrap_or(0),
            "cache_read_input_tokens": cache,
            "cache_creation_input_tokens": 0,
        }
    })
}

/// Texto de un campo `system` (cadena o lista de bloques de texto).
fn texto_de(v: &Value) -> String {
    match v {
        Value::String(t) => t.clone(),
        Value::Array(b) => b
            .iter()
            .filter_map(|x| x["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// Turno del asistente: texto y llamadas a herramientas.
fn asistente(bloques: &[Value]) -> Value {
    let texto: Vec<&str> = bloques
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let llamadas: Vec<Value> = bloques
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| {
            json!({
                "id": b["id"],
                "type": "function",
                "function": { "name": b["name"], "arguments": b["input"].to_string() },
            })
        })
        .collect();
    let mut m = json!({ "role": "assistant", "content": if texto.is_empty() { Value::Null } else { json!(texto.join("\n\n")) } });
    if !llamadas.is_empty() {
        m["tool_calls"] = Value::Array(llamadas);
    }
    m
}

/// Turno del usuario: resultados de herramientas (un mensaje `tool` cada
/// uno) y, aparte, texto, imágenes y PDF.
fn usuario(bloques: &[Value]) -> Vec<Value> {
    let mut v = Vec::new();
    let mut partes = Vec::new();
    for b in bloques {
        match b["type"].as_str() {
            Some("tool_result") => {
                let mut t = texto_de(&b["content"]);
                if b["is_error"] == true {
                    t = format!("ERROR: {t}");
                }
                v.push(json!({ "role": "tool", "tool_call_id": b["tool_use_id"], "content": t }));
            }
            Some("text") => partes.push(json!({ "type": "text", "text": b["text"] })),
            Some("image") => {
                let s = &b["source"];
                partes.push(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", s["media_type"].as_str().unwrap_or("image/png"), s["data"].as_str().unwrap_or("")) }
                }));
            }
            Some("document") => {
                let s = &b["source"];
                partes.push(json!({
                    "type": "file",
                    "file": {
                        "filename": b["title"].as_str().unwrap_or("adjunto.pdf"),
                        "file_data": format!("data:{};base64,{}", s["media_type"].as_str().unwrap_or("application/pdf"), s["data"].as_str().unwrap_or("")),
                    }
                }));
            }
            _ => {}
        }
    }
    if !partes.is_empty() {
        // Solo texto: como cadena (lo aceptan todos los servicios compatibles)
        let solo_texto = partes.iter().all(|p| p["type"] == "text");
        let contenido = if solo_texto {
            json!(
                partes
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            )
        } else {
            Value::Array(partes)
        };
        v.push(json!({ "role": "user", "content": contenido }));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_y_vuelta_con_herramientas() {
        let cuerpo = json!({
            "model": "gpt-x",
            "max_tokens": 1000,
            "system": [{ "type": "text", "text": "Instrucciones", "cache_control": { "type": "ephemeral" } }],
            "tools": [{ "name": "buscar", "description": "Busca", "input_schema": { "type": "object" }, "cache_control": { "type": "ephemeral" } }],
            "messages": [
                { "role": "user", "content": [{ "type": "text", "text": "Hola" }] },
                { "role": "assistant", "content": [
                    { "type": "text", "text": "Miro" },
                    { "type": "tool_use", "id": "t1", "name": "buscar", "input": { "texto": "rejilla" } }
                ]},
                { "role": "user", "content": [
                    { "type": "tool_result", "tool_use_id": "t1", "content": "nada", "is_error": true }
                ]},
            ]
        });
        let p = peticion(&cuerpo, true);
        assert_eq!(p["max_completion_tokens"], 1000);
        assert!(p.get("max_tokens").is_none());
        let m = p["messages"].as_array().unwrap();
        assert_eq!(m[0], json!({ "role": "system", "content": "Instrucciones" }));
        assert_eq!(m[1], json!({ "role": "user", "content": "Hola" }));
        assert_eq!(
            m[2]["tool_calls"][0]["function"]["arguments"],
            "{\"texto\":\"rejilla\"}"
        );
        assert_eq!(
            m[3],
            json!({ "role": "tool", "tool_call_id": "t1", "content": "ERROR: nada" })
        );
        assert_eq!(p["tools"][0]["function"]["name"], "buscar");
        assert!(p["tools"][0].get("cache_control").is_none());
        assert_eq!(peticion(&cuerpo, false)["max_tokens"], 1000);

        let r = respuesta(&json!({
            "choices": [{ "finish_reason": "tool_calls", "message": {
                "content": null,
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "ver_concepto", "arguments": "{\"codigo\":\"VG10\"}" } }]
            }}],
            "usage": { "prompt_tokens": 1200, "completion_tokens": 50, "prompt_tokens_details": { "cached_tokens": 1000 } }
        }));
        assert_eq!(r["stop_reason"], "tool_use");
        assert_eq!(r["content"][0]["input"]["codigo"], "VG10");
        assert_eq!(r["usage"]["input_tokens"], 200);
        assert_eq!(r["usage"]["cache_read_input_tokens"], 1000);
        let fin = respuesta(&json!({ "choices": [{ "finish_reason": "stop", "message": { "content": "Hecho" } }] }));
        assert_eq!(fin["stop_reason"], "end_turn");
        assert_eq!(fin["content"][0]["text"], "Hecho");
    }

    #[test]
    fn imagenes_y_pdf_como_partes() {
        let cuerpo = json!({ "model": "m", "max_tokens": 10, "messages": [
            { "role": "user", "content": [
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "AAA" } },
                { "type": "document", "title": "oferta.pdf", "source": { "type": "base64", "media_type": "application/pdf", "data": "BBB" } },
                { "type": "text", "text": "Valora" }
            ]}
        ]});
        let m = &peticion(&cuerpo, true)["messages"][0]["content"];
        assert_eq!(m[0]["image_url"]["url"], "data:image/png;base64,AAA");
        assert_eq!(m[1]["file"]["filename"], "oferta.pdf");
        assert_eq!(m[2]["text"], "Valora");
    }
}
