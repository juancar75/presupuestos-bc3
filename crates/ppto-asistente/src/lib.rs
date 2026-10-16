// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-asistente
//!
//! Modo **directo** del asistente del programa: el programa habla con la API
//! de mensajes de Anthropic usando la clave del usuario y las mismas
//! herramientas que el servidor MCP (`ppto-mcp`). La IA consulta la obra
//! activa, lee los adjuntos, hace **propuestas** (que el usuario acepta o
//! rechaza) y guarda informes; nunca cambia el presupuesto.
//!
//! El gasto se controla con un límite mensual en dólares; cada respuesta
//! lleva su coste, calculado con los precios por millón de tokens de los
//! ajustes (editables, porque cambian).

use base64::Engine as _;
use ppto_propuestas::conversacion::{Autor, Conversaciones, Mensaje};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const URL_API: &str = "https://api.anthropic.com/v1/messages";
pub const VERSION_API: &str = "2023-06-01";
/// Herramientas del MCP que no tienen sentido en el modo directo.
pub const EXCLUIDAS: &[&str] = &["leer_mensajes", "responder", "abrir_presupuesto", "abrir_ejemplo"];
/// Máximo de vueltas (llamadas a la API) por mensaje.
pub const MAX_VUELTAS: usize = 40;
/// Tamaño máximo de un PDF o imagen adjunto que se envía entero.
const MAX_BINARIO: u64 = 20 * 1024 * 1024;

// ---------------------------------------------------------------- ajustes

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modo {
    /// El mensaje queda pendiente para la IA del usuario por MCP (gratis).
    #[default]
    Encargo,
    /// El programa llama a la API con la clave del usuario.
    Directo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ajustes {
    pub modo: Modo,
    /// Clave de la API. Si está vacía se usa la variable `ANTHROPIC_API_KEY`.
    pub clave_api: String,
    pub modelo: String,
    /// Límite de gasto mensual en dólares.
    pub limite_mensual: Decimal,
    /// Dólares por millón de tokens de entrada y de salida.
    pub precio_entrada: Decimal,
    pub precio_salida: Decimal,
    pub max_tokens: u32,
}

impl Default for Ajustes {
    fn default() -> Self {
        Self {
            modo: Modo::Encargo,
            clave_api: String::new(),
            modelo: "claude-sonnet-5-5".into(),
            limite_mensual: Decimal::from(20),
            precio_entrada: Decimal::from(2),
            precio_salida: Decimal::from(10),
            max_tokens: 16_000,
        }
    }
}

impl Ajustes {
    pub fn ruta() -> Option<PathBuf> {
        ppto_propuestas::carpeta_config().map(|c| c.join("asistente-ajustes.json"))
    }

    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        if let Some(d) = ruta.parent() {
            std::fs::create_dir_all(d)?;
        }
        let t = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(ruta, t)
    }

    /// Clave efectiva (ajustes o variable de entorno).
    pub fn clave(&self) -> Option<String> {
        let k = self.clave_api.trim();
        if k.is_empty() {
            std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty())
        } else {
            Some(k.to_owned())
        }
    }
}

// ---------------------------------------------------------------- gasto

/// Gasto por mes (`"2026-10"` → dólares).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gasto {
    pub meses: BTreeMap<String, Decimal>,
}

impl Gasto {
    pub fn ruta() -> Option<PathBuf> {
        ppto_propuestas::carpeta_config().map(|c| c.join("asistente-gasto.json"))
    }

    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn del_mes(&self, mes: &str) -> Decimal {
        self.meses.get(mes).copied().unwrap_or_default()
    }

    /// Suma un importe al mes y guarda.
    pub fn sumar(ruta: &Path, mes: &str, importe: Decimal) -> std::io::Result<Decimal> {
        ppto_propuestas::con_bloqueo(ruta, || {
            let mut g = Self::cargar(ruta);
            let total = g.del_mes(mes) + importe;
            g.meses.insert(mes.to_owned(), total);
            let t = serde_json::to_string_pretty(&g).map_err(std::io::Error::other)?;
            std::fs::write(ruta, t)?;
            Ok(total)
        })
    }
}

/// Mes actual `AAAA-MM` (UTC).
pub fn mes_actual() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    mes_de(i64::try_from(segundos / 86_400).unwrap_or(0))
}

/// `AAAA-MM` a partir de días desde 1970-01-01 (algoritmo de H. Hinnant).
fn mes_de(dias: i64) -> String {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{a:04}-{m:02}")
}

/// Tokens consumidos en una o varias llamadas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Consumo {
    pub entrada: u64,
    pub salida: u64,
    pub cache_escritura: u64,
    pub cache_lectura: u64,
}

impl Consumo {
    fn de_respuesta(r: &Value) -> Self {
        let u = &r["usage"];
        let n = |c: &str| u[c].as_u64().unwrap_or(0);
        Self {
            entrada: n("input_tokens"),
            salida: n("output_tokens"),
            cache_escritura: n("cache_creation_input_tokens"),
            cache_lectura: n("cache_read_input_tokens"),
        }
    }

    fn sumar(&mut self, o: Self) {
        self.entrada += o.entrada;
        self.salida += o.salida;
        self.cache_escritura += o.cache_escritura;
        self.cache_lectura += o.cache_lectura;
    }

    /// Coste en dólares: la escritura en caché cuesta 1,25 veces la entrada y
    /// la lectura 0,1 veces.
    pub fn coste(&self, a: &Ajustes) -> Decimal {
        let millon = Decimal::from(1_000_000);
        let entrada = Decimal::from(self.entrada)
            + Decimal::from(self.cache_escritura) * Decimal::new(125, 2)
            + Decimal::from(self.cache_lectura) * Decimal::new(1, 1);
        (entrada * a.precio_entrada + Decimal::from(self.salida) * a.precio_salida) / millon
    }
}

/// Importe en dólares para mostrar (`0,0123 $`).
pub fn dolares(d: Decimal) -> String {
    format!("{} $", ppto_core::formato::num(d, 4))
}

// ---------------------------------------------------------------- transporte

/// Envío de una petición a la API (sustituible en las pruebas).
pub trait Transporte {
    fn enviar(&self, cuerpo: &Value) -> Result<Value, String>;
}

/// Transporte HTTPS real con la clave del usuario. Usa los certificados del
/// sistema (funciona con los proxies corporativos que los instalan).
pub struct Http {
    clave: String,
    agente: ureq::Agent,
}

impl Http {
    pub fn new(clave: String) -> Self {
        use ureq::tls::{RootCerts, TlsConfig};
        let agente = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .http_status_as_error(false)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .build()
            .into();
        Self { clave, agente }
    }
}

impl Transporte for Http {
    fn enviar(&self, cuerpo: &Value) -> Result<Value, String> {
        let texto = cuerpo.to_string();
        let mut espera = 5;
        for intento in 0..4 {
            let mut r = self
                .agente
                .post(URL_API)
                .header("x-api-key", &self.clave)
                .header("anthropic-version", VERSION_API)
                .header("content-type", "application/json")
                .send(texto.as_str())
                .map_err(|e| format!("no se pudo conectar con la API: {e}"))?;
            let estado = r.status().as_u16();
            let cuerpo = r
                .body_mut()
                .with_config()
                .limit(50 * 1024 * 1024)
                .read_to_string()
                .map_err(|e| format!("respuesta de la API ilegible: {e}"))?;
            let v: Value = serde_json::from_str(&cuerpo).unwrap_or_else(|_| json!({ "texto": cuerpo }));
            if estado == 200 {
                return Ok(v);
            }
            // Saturación o límite de peticiones: reintentar con espera.
            if matches!(estado, 429 | 500 | 502 | 503 | 529) && intento < 3 {
                std::thread::sleep(std::time::Duration::from_secs(espera));
                espera *= 3;
                continue;
            }
            let m = v["error"]["message"].as_str().unwrap_or(&cuerpo).to_owned();
            return Err(match estado {
                401 => format!("clave de API no válida ({m})"),
                400 if m.contains("credit") => format!("sin saldo en la cuenta de la API ({m})"),
                _ => format!("error {estado} de la API: {m}"),
            });
        }
        Err("la API sigue saturada; vuelva a intentarlo en unos minutos".into())
    }
}

// ---------------------------------------------------------------- conversación

/// Progreso que se va mostrando en el panel.
#[derive(Debug, Clone, PartialEq)]
pub enum Evento {
    Pensando,
    Herramienta(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Respuesta {
    pub texto: String,
    pub consumo: Consumo,
    pub coste: Decimal,
    pub herramientas: usize,
}

pub fn instrucciones(obra: &str) -> String {
    format!(
        "Eres el asistente de un programa de presupuestos de obra e instalaciones (climatización, \
ventilación, PCI, fontanería, electricidad) en España. Hablas en español, con formato español \
de números (1.234,56 €). La obra abierta en el programa es «{obra}»: ábrela con abrir_obra_activa \
antes de valorar.\n\n\
Reglas:\n\
- Nunca cambias el presupuesto. Pones precios con proponer_lote (varias partidas por llamada, recursos comunes definidos una vez) o proponer: una propuesta por partida, siempre \
como descompuesto (material, mano de obra, maquinaria o subcontrata y medios auxiliares), con \
confianza y justificación. El usuario acepta o rechaza cada propuesta en el programa.\n\
- Reutiliza los recursos y precios que ya existen en la obra (buscar, ver_concepto) y las tarifas \
de subcontratas del usuario (buscar_tarifas). Para material de catálogo de un fabricante usa pvp \
y fabricante (y grupo si tiene descuento propio) y estima el descuento del instalador.\n\
- Si te adjuntan ofertas de fabricantes, úsalas como fuente de precios y cítalas en la \
justificación.\n\
- Para encargos de una obra nueva, organiza el presupuesto por capítulos en orden de ejecución y \
no olvides la legalización según la potencia y la normativa (RITE, RIPCI, REBT…): proyecto o \
memoria técnica, certificados, tramitación ante la comunidad autónoma, inspecciones (OCA) y \
documentación final.\n\
- Los informes largos (resúmenes, comparativas, dudas para el cliente) guárdalos con \
guardar_informe.\n\
- Termina con una respuesta breve: qué has hecho (propuestas creadas, informes guardados), dudas \
y supuestos. Si te falta un dato que cambia mucho el precio, pregúntalo."
    )
}

/// Bloques de contenido de los adjuntos: PDF e imágenes enteros (la IA ve
/// tablas y planos), hojas de cálculo y textos como texto.
pub fn bloques_adjuntos(adjuntos: &[String]) -> Vec<Value> {
    let mut v = Vec::new();
    for a in adjuntos {
        let ruta = Path::new(a);
        let nombre = ruta
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = ruta
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let tipo = match ext.as_str() {
            "pdf" => Some("application/pdf"),
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "webp" => Some("image/webp"),
            "gif" => Some("image/gif"),
            _ => None,
        };
        let bloque = match tipo {
            Some(t) => match std::fs::metadata(ruta) {
                Ok(m) if m.len() > MAX_BINARIO => Err(format!("{nombre}: demasiado grande para enviarlo")),
                Ok(_) => std::fs::read(ruta).map_err(|e| format!("{nombre}: {e}")).map(|b| {
                    let datos = base64::engine::general_purpose::STANDARD.encode(b);
                    let clase = if t == "application/pdf" { "document" } else { "image" };
                    let mut b = json!({
                        "type": clase,
                        "source": { "type": "base64", "media_type": t, "data": datos }
                    });
                    if clase == "document" {
                        b["title"] = json!(nombre);
                    }
                    b
                }),
                Err(e) => Err(format!("{nombre}: {e}")),
            },
            None => ppto_mcp::asistente::extraer_texto(ruta)
                .map(|t| json!({ "type": "text", "text": format!("Adjunto «{nombre}» ({a}):\n{t}") })),
        };
        v.push(bloque.unwrap_or_else(|e| json!({ "type": "text", "text": format!("No se pudo adjuntar: {e}") })));
    }
    v
}

/// Historial previo de la obra como mensajes de la API (solo texto),
/// alternando usuario y asistente y empezando por el usuario.
fn historial(previos: &[&Mensaje]) -> Vec<Value> {
    let mut v: Vec<(&str, String)> = Vec::new();
    for m in previos.iter().rev().take(20).rev() {
        let rol = match m.autor {
            Autor::Usuario => "user",
            Autor::Asistente => "assistant",
            Autor::Sistema => continue,
        };
        let mut t = m.texto.clone();
        if !m.adjuntos.is_empty() {
            t.push_str(&format!("\n(Adjuntos entonces: {})", m.adjuntos.join(", ")));
        }
        match v.last_mut() {
            Some((r, x)) if *r == rol => {
                x.push_str("\n\n");
                x.push_str(&t);
            }
            _ => v.push((rol, t)),
        }
    }
    while v.first().is_some_and(|(r, _)| *r != "user") {
        v.remove(0);
    }
    if v.last().is_some_and(|(r, _)| *r == "user") {
        v.pop(); // un mensaje sin respuesta (p. ej. fallido) no se repite
    }
    v.into_iter().map(|(r, t)| json!({ "role": r, "content": t })).collect()
}

/// Marca de caché en el último bloque del último mensaje (y la quita de los
/// anteriores para no pasar de 4 marcas).
fn marcar_cache(mensajes: &mut [Value]) {
    for m in mensajes.iter_mut() {
        if let Some(c) = m["content"].as_array_mut() {
            for b in c {
                if let Some(o) = b.as_object_mut() {
                    o.remove("cache_control");
                }
            }
        }
    }
    if let Some(m) = mensajes.last_mut()
        && let Some(b) = m["content"].as_array_mut().and_then(|c| c.last_mut())
    {
        b["cache_control"] = json!({ "type": "ephemeral" });
    }
}

/// Atiende un mensaje del usuario: bucle de herramientas hasta la respuesta
/// final. `gastado` es lo ya gastado este mes (para respetar el límite).
#[allow(clippy::too_many_arguments)]
pub fn atender(
    transporte: &dyn Transporte,
    ajustes: &Ajustes,
    estado: &mut ppto_mcp::Estado,
    obra: &str,
    previos: &[&Mensaje],
    texto: &str,
    adjuntos: &[String],
    gastado: Decimal,
    avisar: &mut dyn FnMut(Evento),
) -> Result<Respuesta, String> {
    let mut herramientas = ppto_mcp::herramientas_api(EXCLUIDAS);
    if let Some(h) = herramientas.last_mut() {
        h["cache_control"] = json!({ "type": "ephemeral" });
    }
    let sistema = json!([{ "type": "text", "text": instrucciones(obra), "cache_control": { "type": "ephemeral" } }]);
    let mut mensajes = historial(previos);
    let mut contenido = bloques_adjuntos(adjuntos);
    contenido.push(json!({ "type": "text", "text": texto }));
    mensajes.push(json!({ "role": "user", "content": contenido }));

    let mut consumo = Consumo::default();
    let mut usadas = 0;
    for _ in 0..MAX_VUELTAS {
        let coste = consumo.coste(ajustes);
        if gastado + coste >= ajustes.limite_mensual {
            return Err(format!(
                "alcanzado el límite de gasto mensual ({}); súbalo en los ajustes del asistente",
                dolares(ajustes.limite_mensual)
            ));
        }
        marcar_cache(&mut mensajes);
        avisar(Evento::Pensando);
        let r = transporte.enviar(&json!({
            "model": ajustes.modelo,
            "max_tokens": ajustes.max_tokens,
            "system": sistema,
            "tools": herramientas,
            "messages": mensajes,
        }))?;
        consumo.sumar(Consumo::de_respuesta(&r));
        let bloques = r["content"].as_array().cloned().unwrap_or_default();
        mensajes.push(json!({ "role": "assistant", "content": bloques }));
        if r["stop_reason"] != "tool_use" {
            let texto: Vec<&str> = bloques
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            let mut texto = texto.join("\n\n").trim().to_owned();
            if r["stop_reason"] == "max_tokens" {
                texto.push_str("\n\n(Respuesta cortada por longitud.)");
            }
            if texto.is_empty() {
                texto = "(Sin respuesta de texto.)".into();
            }
            return Ok(Respuesta {
                texto,
                consumo,
                coste: consumo.coste(ajustes),
                herramientas: usadas,
            });
        }
        let mut resultados = Vec::new();
        for b in bloques.iter().filter(|b| b["type"] == "tool_use") {
            let nombre = b["name"].as_str().unwrap_or("");
            avisar(Evento::Herramienta(nombre.to_owned()));
            usadas += 1;
            let (salida, error) = match ppto_mcp::ejecutar(estado, nombre, &b["input"]) {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            resultados.push(json!({
                "type": "tool_result",
                "tool_use_id": b["id"],
                "content": salida,
                "is_error": error,
            }));
        }
        mensajes.push(json!({ "role": "user", "content": resultados }));
    }
    Err(format!(
        "el asistente no terminó en {MAX_VUELTAS} pasos; pida algo más concreto"
    ))
}

/// Atiende el último mensaje pendiente de la obra en el modo directo y
/// guarda la respuesta (o el error) en la conversación y el gasto del mes.
pub fn atender_pendiente(
    transporte: &dyn Transporte,
    ajustes: &Ajustes,
    rutas: &Rutas,
    obra: &str,
    avisar: &mut dyn FnMut(Evento),
) -> Result<Respuesta, String> {
    let conv = Conversaciones::cargar(&rutas.conversaciones);
    let pendientes: Vec<Mensaje> = conv.pendientes(obra).into_iter().cloned().collect();
    if pendientes.is_empty() {
        return Err("no hay mensajes pendientes".into());
    }
    let previos: Vec<&Mensaje> = conv.de_obra(obra).into_iter().filter(|m| m.atendido).collect();
    // Si hay varios pendientes, se juntan en uno.
    let texto = pendientes
        .iter()
        .map(|m| m.texto.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let adjuntos: Vec<String> = pendientes.iter().flat_map(|m| m.adjuntos.clone()).collect();
    let mut estado = ppto_mcp::Estado {
        ruta_obra_activa: Some(rutas.obra_activa.clone()),
        ruta_bandeja: Some(rutas.bandeja.clone()),
        ruta_tarifas: Some(rutas.tarifas.clone()),
        ruta_conversaciones: Some(rutas.conversaciones.clone()),
        ..ppto_mcp::Estado::default()
    };
    let mes = mes_actual();
    let gastado = Gasto::cargar(&rutas.gasto).del_mes(&mes);
    let ids: Vec<u64> = pendientes.iter().map(|m| m.id).collect();
    let r = atender(
        transporte,
        ajustes,
        &mut estado,
        obra,
        &previos,
        &texto,
        &adjuntos,
        gastado,
        avisar,
    );
    match &r {
        Ok(resp) => {
            let total = Gasto::sumar(&rutas.gasto, &mes, resp.coste).unwrap_or(gastado + resp.coste);
            let coste = format!(
                "{} · {} herramientas · mes {}",
                dolares(resp.coste),
                resp.herramientas,
                dolares(total)
            );
            Conversaciones::modificar(&rutas.conversaciones, |c| {
                c.responder(obra, &ids, &resp.texto, Some(coste))
            })
            .map_err(|e| e.to_string())?;
        }
        Err(e) => {
            Conversaciones::modificar(&rutas.conversaciones, |c| {
                c.anadir(obra, Autor::Sistema, &format!("No se pudo atender: {e}"), Vec::new())
            })
            .map_err(|e| e.to_string())?;
        }
    }
    r
}

/// Ficheros compartidos que usa el modo directo.
#[derive(Debug, Clone)]
pub struct Rutas {
    pub conversaciones: PathBuf,
    pub obra_activa: PathBuf,
    pub bandeja: PathBuf,
    pub tarifas: PathBuf,
    pub gasto: PathBuf,
}

impl Rutas {
    /// Rutas en la carpeta de configuración del usuario.
    pub fn de_config() -> Option<Self> {
        Some(Self::en(&ppto_propuestas::carpeta_config()?))
    }

    pub fn en(carpeta: &Path) -> Self {
        Self {
            conversaciones: carpeta.join("asistente.json"),
            obra_activa: carpeta.join("obra-activa.json"),
            bandeja: carpeta.join("propuestas.json"),
            tarifas: carpeta.join("tarifas.json"),
            gasto: carpeta.join("asistente-gasto.json"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn meses() {
        assert_eq!(mes_de(0), "1970-01");
        assert_eq!(mes_de(20_738), "2026-10"); // 2026-10-12
        assert_eq!(mes_de(20_727), "2026-10"); // 2026-10-01
        assert_eq!(mes_de(20_726), "2026-09");
        assert_eq!(mes_de(11_016), "2000-02"); // 2000-02-29
    }

    #[test]
    fn coste_con_cache() {
        let a = Ajustes::default(); // 2 $ / 10 $ por millón
        let c = Consumo {
            entrada: 10_000,
            salida: 2_000,
            cache_escritura: 8_000,
            cache_lectura: 100_000,
        };
        // (10.000 + 10.000 + 10.000) × 2 + 2.000 × 10 = 80.000 / 1e6 = 0,08
        assert_eq!(c.coste(&a), Decimal::new(8, 2));
    }

    /// Transporte de prueba: respuestas en orden y registro de peticiones.
    struct Guion {
        respuestas: RefCell<Vec<Value>>,
        peticiones: RefCell<Vec<Value>>,
    }

    impl Transporte for Guion {
        fn enviar(&self, cuerpo: &Value) -> Result<Value, String> {
            self.peticiones.borrow_mut().push(cuerpo.clone());
            let mut r = self.respuestas.borrow_mut();
            if r.is_empty() {
                Err("sin más respuestas".into())
            } else {
                Ok(r.remove(0))
            }
        }
    }

    fn uso() -> Value {
        json!({ "input_tokens": 1000, "output_tokens": 100 })
    }

    #[test]
    fn bucle_de_herramientas_hasta_la_respuesta() {
        let dir = std::env::temp_dir().join(format!("asis-bucle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rutas = Rutas::en(&dir);
        let mut p = ppto_core::ejemplos::suelo_radiante();
        p.nueva_partida("C01", "P.VAL", "ud", "Válvula de zona").unwrap();
        let obra = ppto_propuestas::clave(&p);
        ppto_propuestas::ObraActiva::guardar(&p, &rutas.obra_activa).unwrap();
        let oferta = dir.join("oferta.csv");
        std::fs::write(&oferta, "Ref;PVP\nVZ-1;30\n").unwrap();
        Conversaciones::modificar(&rutas.conversaciones, |c| {
            c.anadir(
                &obra,
                Autor::Usuario,
                "Valora la válvula",
                vec![oferta.display().to_string()],
            )
        })
        .unwrap();

        let guion = Guion {
            respuestas: RefCell::new(vec![
                json!({ "stop_reason": "tool_use", "usage": uso(), "content": [
                    { "type": "text", "text": "Abro la obra." },
                    { "type": "tool_use", "id": "t1", "name": "abrir_obra_activa", "input": {} },
                ]}),
                json!({ "stop_reason": "tool_use", "usage": uso(), "content": [
                    { "type": "tool_use", "id": "t2", "name": "proponer", "input": {
                        "partida": "P.VAL",
                        "lineas": [
                            { "codigo": "VZ-1", "rendimiento": 1, "naturaleza": "material", "unidad": "ud", "resumen": "Válvula", "precio": 30 }
                        ],
                        "confianza": "alta",
                        "justificacion": "oferta adjunta"
                    }},
                    { "type": "tool_use", "id": "t3", "name": "no_existe", "input": {} },
                ]}),
                json!({ "stop_reason": "end_turn", "usage": uso(), "content": [
                    { "type": "text", "text": "Propuesta 1 creada para P.VAL." }
                ]}),
            ]),
            peticiones: RefCell::new(Vec::new()),
        };
        let a = Ajustes {
            modo: Modo::Directo,
            ..Ajustes::default()
        };
        let mut eventos = Vec::new();
        let r = atender_pendiente(&guion, &a, &rutas, &obra, &mut |e| eventos.push(e)).unwrap();
        assert_eq!(r.texto, "Propuesta 1 creada para P.VAL.");
        assert_eq!(r.herramientas, 3);
        assert!(eventos.contains(&Evento::Herramienta("proponer".into())));

        // La propuesta está en la bandeja, pendiente de aprobación
        let b = ppto_propuestas::Bandeja::cargar(&rutas.bandeja);
        assert_eq!(b.pendientes(&obra).len(), 1);

        // Peticiones: adjunto como texto, herramientas sin las de encargo,
        // errores de herramienta devueltos a la IA y caché marcada.
        let pet = guion.peticiones.borrow();
        assert_eq!(pet.len(), 3);
        let primero = &pet[0]["messages"][0]["content"];
        assert!(primero[0]["text"].as_str().unwrap().contains("VZ-1;30"));
        assert_eq!(primero[1]["text"], "Valora la válvula");
        assert!(
            !pet[0]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["name"] == "responder")
        );
        let ultimo = pet[2]["messages"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(ultimo["content"][1]["is_error"], true);
        assert!(ultimo["content"][1]["cache_control"].is_object());
        let marcas = pet[2].to_string().matches("cache_control").count();
        assert!(marcas <= 4, "{marcas} marcas de caché");

        // Respuesta guardada con su coste; gasto del mes acumulado
        let c = Conversaciones::cargar(&rutas.conversaciones);
        assert!(c.pendientes(&obra).is_empty());
        let resp = c.de_obra(&obra).last().copied().cloned().unwrap();
        assert_eq!(resp.autor, Autor::Asistente);
        // 3 × (1000 × 2 + 100 × 10) / 1e6 = 0,009 $
        assert!(resp.coste.unwrap().starts_with("0,0090 $"));
        assert_eq!(Gasto::cargar(&rutas.gasto).del_mes(&mes_actual()), Decimal::new(9, 3));

        // Segundo mensaje: el historial va como texto y respeta el límite
        Conversaciones::modificar(&rutas.conversaciones, |c| {
            c.anadir(&obra, Autor::Usuario, "¿y el IVA?", vec![])
        })
        .unwrap();
        let tope = Ajustes {
            limite_mensual: Decimal::new(5, 3),
            ..a.clone()
        };
        let e = atender_pendiente(&guion, &tope, &rutas, &obra, &mut |_| {}).unwrap_err();
        assert!(e.contains("límite de gasto"), "{e}");
        let c = Conversaciones::cargar(&rutas.conversaciones);
        assert_eq!(c.de_obra(&obra).last().unwrap().autor, Autor::Sistema);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn historial_alterna_y_empieza_por_usuario() {
        let m = |id, autor, t: &str| Mensaje {
            id,
            obra: "O".into(),
            autor,
            texto: t.into(),
            adjuntos: vec![],
            creado: 0,
            atendido: true,
            coste: None,
        };
        let v = [
            m(1, Autor::Asistente, "hola"),
            m(2, Autor::Usuario, "a"),
            m(3, Autor::Usuario, "b"),
            m(4, Autor::Sistema, "error"),
            m(5, Autor::Asistente, "c"),
            m(6, Autor::Usuario, "sin respuesta"),
        ];
        let refs: Vec<&Mensaje> = v.iter().collect();
        let h = historial(&refs);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0]["content"], "a\n\nb");
        assert_eq!(h[1]["role"], "assistant");
    }
}
