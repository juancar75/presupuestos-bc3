// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-mcp
//!
//! Servidor MCP (Model Context Protocol, JSON-RPC 2.0 por stdio) para que un
//! asistente como Claude consulte y simule presupuestos (P-020).
//!
//! **Ninguna herramienta modifica el presupuesto**. Las simulaciones trabajan
//! sobre copias y las exportaciones crean ficheros nuevos. Para poner
//! precios, la IA abre la obra activa del programa (`abrir_obra_activa`) y
//! hace **propuestas** (`proponer`) que el usuario acepta o rechaza en el
//! programa de escritorio (P-021).
//!
//! Todas las cifras salen de `ppto-core`, igual que en la interfaz y los informes.

#![recursion_limit = "512"]

use ppto_core::concepto::Naturaleza;
use ppto_core::formato::{eur, num};
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, simular};
use ppto_core::{Decimal, Presupuesto, venta};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::str::FromStr;

pub mod asistente;
mod biblioteca;
mod documentos;
pub mod dxf;
pub mod ifc;

/// Versión del protocolo MCP que se anuncia si el cliente no pide otra.
pub const PROTOCOLO: &str = "2025-06-18";

/// Presupuesto abierto y porcentajes del resumen.
pub struct Estado {
    pub presupuesto: Option<Presupuesto>,
    pub origen: String,
    /// Clave de la obra activa del programa (si se abrió con `abrir_obra_activa`).
    pub obra_activa: Option<String>,
    /// Ficheros compartidos con el programa (por defecto, en la carpeta de configuración).
    pub ruta_obra_activa: Option<std::path::PathBuf>,
    pub ruta_bandeja: Option<std::path::PathBuf>,
    pub ruta_tarifas: Option<std::path::PathBuf>,
    pub ruta_conversaciones: Option<std::path::PathBuf>,
    pub ruta_biblioteca: Option<std::path::PathBuf>,
    /// Datos de la obra propuestos por la IA (`ficha_propuesta.json`).
    pub ruta_ficha: Option<std::path::PathBuf>,
    pub gg: Decimal,
    pub bi: Decimal,
    pub iva: Decimal,
    /// Quién firma las propuestas: `PPTO_AUTOR` o el cliente MCP que se
    /// conecta (Claude, ChatGPT/Codex…).
    pub autor: String,
}

impl Default for Estado {
    fn default() -> Self {
        Self {
            presupuesto: None,
            origen: String::new(),
            obra_activa: None,
            ruta_obra_activa: ppto_propuestas::ruta_obra_activa(),
            ruta_bandeja: ppto_propuestas::ruta_bandeja(),
            ruta_tarifas: ppto_propuestas::carpeta_config().map(|c| c.join("tarifas.json")),
            ruta_conversaciones: ppto_propuestas::conversacion::ruta_conversaciones(),
            ruta_ficha: ppto_propuestas::ruta_ficha_propuesta(),
            ruta_biblioteca: ppto_biblioteca::ruta(),
            gg: venta::GG_HABITUAL,
            bi: venta::BI_HABITUAL,
            iva: venta::IVA_GENERAL,
            autor: std::env::var("PPTO_AUTOR")
                .ok()
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_else(|| "IA".into()),
        }
    }
}

impl Estado {
    /// Abre un `.bc3` (importación) o un `.sqlite` (última revisión). Devuelve un informe.
    pub fn abrir(&mut self, ruta: &str) -> Result<String, String> {
        let ruta = ruta.trim().trim_matches('"');
        let ext = std::path::Path::new(ruta)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut informe = String::new();
        if ext == "bc3" {
            let imp = ppto_bc3::importar_fichero(ruta).map_err(|e| e.to_string())?;
            let k = &imp.porcentajes;
            self.gg = k.gastos_generales.unwrap_or(venta::GG_HABITUAL);
            self.bi = k.beneficio_industrial.unwrap_or(venta::BI_HABITUAL);
            self.iva = k.iva.unwrap_or(venta::IVA_GENERAL);
            let _ = writeln!(
                informe,
                "BC3 importado ({} · {}): {} conceptos, {} errores, {} avisos, {} precios que no cuadran con el fichero.",
                imp.cabecera.programa,
                imp.cabecera.version_formato,
                imp.presupuesto.conceptos.len(),
                imp.errores(),
                imp.avisos(),
                imp.discrepancias.len()
            );
            for i in imp
                .incidencias
                .iter()
                .filter(|i| i.gravedad != ppto_bc3::Gravedad::Info)
                .take(15)
            {
                let _ = writeln!(informe, "- {i}");
            }
            for d in imp.discrepancias.iter().take(10) {
                let _ = writeln!(
                    informe,
                    "- {} declarado {} € / calculado {} €",
                    d.codigo,
                    eur(d.declarado),
                    eur(d.calculado)
                );
            }
            self.presupuesto = Some(imp.presupuesto);
        } else if ext == "sqlite" || ext == "db" {
            let a = ppto_db::Almacen::abrir(ruta).map_err(|e| e.to_string())?;
            let revs = a.listar_revisiones().map_err(|e| e.to_string())?;
            let ultima = revs.last().ok_or("el fichero no contiene revisiones")?;
            self.presupuesto = Some(a.cargar_revision(ultima.id).map_err(|e| e.to_string())?);
            let _ = writeln!(
                informe,
                "Abierta la revisión {} ({}, {}).",
                ultima.etiqueta, ultima.autor, ultima.creado_en
            );
        } else {
            return Err(format!("extensión no admitida «{ext}»: use .bc3 o .sqlite"));
        }
        self.origen = ruta.to_owned();
        self.obra_activa = None;
        let p = self.presupuesto.as_ref().ok_or("sin presupuesto")?;
        let _ = writeln!(
            informe,
            "{} — PEM {} €",
            p.nombre,
            eur(p.pem().map_err(|e| e.to_string())?)
        );
        Ok(informe)
    }

    pub(crate) fn p(&self) -> Result<&Presupuesto, String> {
        self.presupuesto
            .as_ref()
            .ok_or_else(|| "no hay presupuesto abierto: use abrir_presupuesto (o abrir_ejemplo)".to_owned())
    }
}

// ---------------------------------------------------------------- protocolo

/// Procesa una línea JSON-RPC y devuelve la respuesta (o nada si es una notificación).
pub fn procesar_linea(estado: &mut Estado, linea: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(linea) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("JSON no válido: {e}")).to_string()),
    };
    procesar(estado, &msg).map(|v| v.to_string())
}

pub fn procesar(estado: &mut Estado, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let metodo = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        return None; // notificación (p. ej. notifications/initialized)
    };
    let r = match metodo {
        "initialize" => {
            if std::env::var("PPTO_AUTOR").map_or(true, |a| a.trim().is_empty())
                && let Some(n) = params.pointer("/clientInfo/name").and_then(Value::as_str)
            {
                estado.autor = ppto_propuestas::autor_de_cliente(n);
            }
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOLO);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "presupuestos-bc3", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Gestor de presupuestos de construcción e instalaciones (BC3/FIEBDC-3). \
                    Abre un presupuesto con abrir_presupuesto y consulta o simula. Las herramientas no \
                    modifican el presupuesto. Para valorar la obra que el usuario tiene abierta en el \
                    programa: abrir_obra_activa, antes de valorar ver_datos_obra (si faltan datos y hay carpeta de documentos: documentos_obra, leer_documento de la memoria y el pliego, y proponer_datos_obra; use también los documentos para marcas, modelos y alcance) y comprobar_recursos (avise al usuario de los huecos: fabricantes sin tarifa o sin descuento, nombres distintos, marcas sin fuente), partidas_sin_precio, ver_concepto/buscar/buscar_tarifas \
                    para reutilizar recursos y precios existentes; ANTES de estimar consulte buscar_biblioteca (partidas, tarifas y ofertas del usuario de obras anteriores), y proponer (una propuesta por partida; para varias partidas, proponer_lote, que acepta hasta 40 en una llamada), \
                    con confianza y justificación de cada precio y rendimiento; el material de catálogo de fabricante o \
                    distribuidor siempre a PVP con fabricante y descuento, nunca a neto). Para quitar partidas que no proceden, \
                    cambiar cantidades o mediciones, rehacer resúmenes y textos o mover partidas de capítulo: \
                    proponer_cambios (todo en una propuesta). El usuario acepta o \
                    rechaza cada propuesta en el programa; consulte ver_propuestas. Si trabajan varios agentes \
                    (Claude, ChatGPT…), ver_propuestas muestra quién propuso cada cosa y los desacuerdos; para \
                    revisar la propuesta de otro agente use comentar_propuesta (de acuerdo o en contra, con el \
                    motivo) o haga su propia propuesta para esa partida. Si el usuario \
                    pide «atiende el asistente» o similar, lea los encargos que dejó en el panel del \
                    programa con leer_mensajes (adjuntos con leer_adjunto), atiéndalos y conteste con \
                    responder; los informes se guardan con guardar_informe. Antes de dar por buena una oferta, comparar_obras con las obras parecidas. Cuando el usuario pase precios ciertos (Saltoki, tarifas, ofertas), guárdelos con guardar_precios para que la base común crezca. Importes en euros con \
                    formato español."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": herramientas() })),
        "tools/call" => {
            let nombre = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (texto, es_error) = match llamar(estado, nombre, &args) {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            Ok(json!({ "content": [{ "type": "text", "text": texto }], "isError": es_error }))
        }
        otro => Err((-32601, format!("método no admitido: {otro}"))),
    };
    Some(match r {
        Ok(resultado) => json!({ "jsonrpc": "2.0", "id": id, "result": resultado }),
        Err((codigo, m)) => error(id, codigo, &m),
    })
}

fn error(id: Value, codigo: i64, mensaje: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": codigo, "message": mensaje } })
}

// ---------------------------------------------------------------- herramientas

fn herramientas() -> Value {
    let num = json!({ "type": ["number", "string"] });
    json!([
        {
            "name": "abrir_presupuesto",
            "description": "Abre un presupuesto: fichero .bc3 (FIEBDC-3, p. ej. exportado de Presto) o .sqlite del programa. Devuelve un informe de importación.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string", "description": "Ruta completa del fichero" } }, "required": ["ruta"] }
        },
        {
            "name": "abrir_ejemplo",
            "description": "Abre el presupuesto sintético de ejemplo (suelo radiante) para probar las herramientas.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "resumen",
            "description": "Resumen del presupuesto abierto: capítulos con importe y porcentaje, coste directo, costes indirectos, PEM, GG, BI, IVA y total.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "buscar",
            "description": "Busca conceptos (partidas, recursos, capítulos) por código, resumen o texto descriptivo. Sin distinguir mayúsculas ni tildes.",
            "inputSchema": { "type": "object", "properties": {
                "texto": { "type": "string" },
                "limite": { "type": "integer", "description": "Máximo de resultados (por defecto 30)" }
            }, "required": ["texto"] }
        },
        {
            "name": "ver_concepto",
            "description": "Detalle de un concepto: precio (coste y con indirectos), descompuesto, capítulos donde aparece con su cantidad, mediciones y texto descriptivo.",
            "inputSchema": { "type": "object", "properties": { "codigo": { "type": "string" } }, "required": ["codigo"] }
        },
        {
            "name": "recursos",
            "description": "Recursos necesarios para ejecutar la obra (cantidades totales e importes) y horas por oficio. Opcionalmente filtra por naturaleza.",
            "inputSchema": { "type": "object", "properties": {
                "naturaleza": { "type": "string", "enum": ["mano_obra", "material", "maquinaria", "subcontrata", "otros"] }
            } }
        },
        {
            "name": "simular_cambios",
            "description": "Simula cambios sin modificar el presupuesto: porcentaje de costes indirectos, precios de recursos y cantidades de partidas. Devuelve el PEM antes y después y las partidas más afectadas.",
            "inputSchema": { "type": "object", "properties": {
                "costes_indirectos": num,
                "precios": { "type": "array", "items": { "type": "object", "properties": { "codigo": { "type": "string" }, "precio": num }, "required": ["codigo", "precio"] } },
                "cantidades": { "type": "array", "items": { "type": "object", "properties": { "capitulo": { "type": "string" }, "partida": { "type": "string" }, "cantidad": num }, "required": ["capitulo", "partida", "cantidad"] } }
            } }
        },
        {
            "name": "simular_subcontrata",
            "description": "Simula subcontratar parte de una partida (por defecto su mano de obra) por unidad de contratación (con factor de conversión) o a precio alzado. Devuelve coste propio retirado, coste contratado, precio equivalente, ahorro y horas liberadas.",
            "inputSchema": { "type": "object", "properties": {
                "partida": { "type": "string" },
                "recursos": { "type": "array", "items": { "type": "string" }, "description": "Códigos a sustituir; por defecto, la mano de obra de la partida" },
                "modalidad": { "type": "string", "enum": ["unidad", "alzado"] },
                "precio": num,
                "unidad": { "type": "string" },
                "factor": num,
                "importe": num
            }, "required": ["partida", "modalidad"] }
        },
        {
            "name": "abrir_obra_activa",
            "description": "Abre el presupuesto que el usuario tiene abierto en el programa de escritorio (p. ej. un ciego que está valorando). Necesario para hacer propuestas.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "ver_datos_obra",
            "description": "Datos de la obra activa: dirección, cliente, carpeta de documentos y la ficha del edificio que piden sus especialidades (superficies, plantas sobre y bajo rasante, viviendas, plazas de garaje, escaleras…), con su clave. Los vacíos salen «—».",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "documentos_obra",
            "description": "Lista los ficheros de la carpeta de documentos de la obra (planos, memorias, pliegos, cálculos) y cuáles se pueden leer.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "leer_documento",
            "description": "Texto de un documento de la carpeta de la obra (PDF, Word, Excel, CSV, TXT), por trozos de 60.000 caracteres: si sigue, repita con «desde» que se indica.",
            "inputSchema": { "type": "object", "properties": {
                "ruta": { "type": "string", "description": "Tal cual sale en documentos_obra" },
                "desde": { "type": "integer" }
            }, "required": ["ruta"] }
        },
        {
            "name": "leer_plano",
            "description": "Lee un plano DXF de la carpeta de la obra: bloques insertados por nombre y capa (cuántos detectores, BIE, extintores, rejillas…), los textos (leyendas, rótulos) y, con «bloque», la distancia de cada uno al más cercano (mín., mediana, máx., en m) y la superficie que ocupan, para comprobar coberturas frente a la medición y la norma. Los DWG se convierten solos a DXF si está LibreDWG (dwg2dxf.exe junto al programa); si no, guardarlos como DXF. Los modelos IFC (BIM) se leen también: elementos de instalaciones por clase, tipo, tamaño y planta, con uds y metros de conducto y tubería si el modelo trae la longitud, otros elementos por clase y m² de espacios por planta; en IFC «bloque» filtra por clase, tipo o tamaño.",
            "inputSchema": { "type": "object", "properties": {
                "ruta": { "type": "string", "description": "Tal cual sale en documentos_obra" },
                "bloque": { "type": "string", "description": "DXF/DWG: nombre del bloque para distancias y cobertura. IFC: texto para filtrar (clase, tipo o tamaño)" }
            }, "required": ["ruta"] }
        },
        {
            "name": "proponer_datos_obra",
            "description": "Propone datos de la obra sacados de sus documentos (claves de ver_datos_obra): números en las de la ficha (m², ud, m), texto en dirección, cliente, etc. Cada dato con su fuente («Memoria descriptiva, pág. 3»). NO cambia la obra: el usuario los acepta en Obra → Datos de la obra. No proponga lo que no esté en un documento.",
            "inputSchema": { "type": "object", "properties": {
                "datos": { "type": "array", "items": { "type": "object", "properties": {
                    "clave": { "type": "string" },
                    "valor": { "type": ["string", "number"] },
                    "fuente": { "type": "string" }
                }, "required": ["clave", "valor", "fuente"] } }
            }, "required": ["datos"] }
        },
        {
            "name": "guardar_precios",
            "description": "Guarda en la biblioteca compartida precios sueltos que el usuario le ha dado (consulta a Saltoki Online, web o tarifa de un fabricante, oferta): PVP por referencia, con su fuente y su origen (real = cierto, por defecto; revisado; estimado). Se añaden a la tarifa del año sin tocar el resto, con quién y cuándo, y los ve el compañero. Úselo SIEMPRE que el usuario pase precios ciertos, para que la base crezca. Para Saltoki: fabricante «SALTOKI», referencia = código Saltoki y grupo = familia de descuento.",
            "inputSchema": { "type": "object", "properties": {
                "fabricante": { "type": "string" },
                "etiqueta": { "type": "string", "description": "Tarifa a la que se añaden (por defecto el año en curso)" },
                "fuente": { "type": "string", "description": "p. ej. «Saltoki Online 19-11-2026»" },
                "origen": { "type": "string", "enum": ["real", "revisado", "estimado"] },
                "lineas": { "type": "array", "items": { "type": "object", "properties": {
                    "referencia": { "type": "string" },
                    "descripcion": { "type": "string" },
                    "unidad": { "type": "string" },
                    "pvp": num,
                    "grupo": { "type": "string" }
                }, "required": ["referencia", "pvp"] } }
            }, "required": ["fabricante", "fuente", "lineas"] }
        },
        {
            "name": "comparar_obras",
            "description": "Compara la obra activa con las obras parecidas guardadas en la biblioteca (mismo tipo y tamaño parecido): PEM €/plaza, €/m², horas de mano de obra por plaza o vivienda, importe por grupo de capítulo (ventilación, detección, BIE…) y reparto de costes, con mediana, mínimo, máximo y desviación. Úselo antes de dar la oferta por buena y explique las desviaciones de más del 25 %.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "comprobar_recursos",
            "description": "PRIMER PASO antes de valorar: lee la obra activa y comprueba si están los recursos para sacar los precios. Por fabricante de la obra: tarifa en la carpeta de tarifas o en la biblioteca, descuento en la obra o en la biblioteca, nombres parecidos (el mismo fabricante con otro nombre). Marcas de la biblioteca o la carpeta que se nombran en partidas sin precio y si tienen tarifa y descuento. Materiales con neto fijo, con PVP distinto del de la biblioteca y recursos a 0 €. Avise al usuario de lo marcado FALTA antes de proponer precios.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "partidas_sin_precio",
            "description": "Partidas de la obra con precio 0 (por valorar), con unidad, cantidad, capítulo y texto descriptivo.",
            "inputSchema": { "type": "object", "properties": { "limite": { "type": "integer", "description": "Máximo (por defecto 40)" } } }
        },
        {
            "name": "buscar_tarifas",
            "description": "Busca en las tarifas de subcontratas del usuario (precios netos sin IVA por unidad de obra). Devuelve el código a usar en un descompuesto (PROVEEDOR.REFERENCIA), unidad y precio.",
            "inputSchema": { "type": "object", "properties": { "texto": { "type": "string" } }, "required": ["texto"] }
        },
        {
            "name": "proponer",
            "description": "Propone el precio de una partida de la obra activa como DESCOMPUESTO (lineas): siempre material, mano de obra, maquinaria o subcontrata y medios auxiliares; no se admiten precios directos. NO modifica el presupuesto: queda pendiente hasta que el usuario la acepte en el programa. Reutilice los códigos de recursos que ya existen en la obra (se usan con su precio de la obra); para un recurso nuevo indique naturaleza, unidad, resumen y precio. Material de catálogo de un fabricante o distribuidor (Saltoki, etc.): SIEMPRE a PVP, nunca a neto, porque el usuario quiere ver PVP − descuento en cada recurso. Indique pvp (precio de tarifa) y fabricante (y grupo si la familia tiene descuento propio, p. ej. SALTOKI/COBRE) en lugar de precio, y en descuentos el descuento del instalador; si solo conoce neto y descuento, pvp = neto / (1 − descuento). El neto se calcula con el descuento de la obra y el usuario lo ajusta después para todos los recursos del fabricante. Si un material ya está en la obra a neto, al darle pvp y fabricante se liga a la tarifa. La respuesta avisa de los materiales que quedan a neto (salvo los marcados «(estimado)»). Para usar una tarifa de subcontrata, código PROVEEDOR.REFERENCIA con naturaleza subcontrata y su precio. Una línea porcentual (medios auxiliares) lleva naturaleza porcentaje y los puntos como rendimiento.",
            "inputSchema": { "type": "object", "properties": {
                "partida": { "type": "string" },
                "lineas": { "type": "array", "items": { "type": "object", "properties": {
                    "codigo": { "type": "string" },
                    "rendimiento": num,
                    "naturaleza": { "type": "string", "enum": ["mano_obra", "material", "maquinaria", "subcontrata", "otros", "porcentaje"] },
                    "unidad": { "type": "string" },
                    "resumen": { "type": "string" },
                    "precio": num,
                    "pvp": num,
                    "fabricante": { "type": "string" },
                    "grupo": { "type": "string", "description": "Grupo de producto del fabricante con descuento propio (p. ej. E08 de S&P)" }
                }, "required": ["codigo", "rendimiento"] } },
                "descuentos": { "type": "array", "description": "Descuento estimado del instalador sobre tarifa por fabricante, o por grupo de producto con fabricante «S&P/E08» (solo se usa si la obra no tiene ya ese descuento)", "items": { "type": "object", "properties": { "fabricante": { "type": "string" }, "porcentaje": num }, "required": ["fabricante", "porcentaje"] } },
                "confianza": { "type": "string", "enum": ["alta", "media", "baja"] },
                "justificacion": { "type": "string", "description": "De dónde salen precios y rendimientos (obra anterior, tarifa, estimación…)" },
                "nueva": { "type": "object", "description": "Solo para una partida que NO está en el presupuesto (contradictorio o adicional): se crea al aceptar, con su capítulo si falta (colocado detrás de despues_de). Código de partida nuevo en «partida».", "properties": {
                    "capitulo": { "type": "string" },
                    "capitulo_resumen": { "type": "string" },
                    "despues_de": { "type": "string", "description": "Capítulo de primer nivel tras el que va el capítulo nuevo" },
                    "unidad": { "type": "string" },
                    "resumen": { "type": "string" },
                    "texto": { "type": "string" },
                    "cantidad": num
                }, "required": ["capitulo", "unidad", "resumen", "cantidad"] }
            }, "required": ["partida", "lineas", "confianza", "justificacion"] }
        },
        {
            "name": "proponer_lote",
            "description": "Igual que proponer, pero para muchas partidas en UNA llamada (recomendado a partir de 3 partidas; hasta 40 por llamada). Los recursos comunes (mano de obra, materiales, fabricante, pvp) se definen una vez en «recursos» y en cada propuesta las líneas solo llevan código y rendimiento, como objeto {codigo, rendimiento} o como par [\"CODIGO\", rendimiento]. «medios_auxiliares» (puntos de %) añade la línea %MA a las propuestas que no la traigan. «confianza» y «justificacion» del lote valen para las propuestas que no indiquen las suyas. Cada propuesta se valida por separado: las que fallan se informan y el resto se guarda.",
            "inputSchema": { "type": "object", "properties": {
                "recursos": { "type": "array", "description": "Definición de recursos nuevos usados en el lote", "items": { "type": "object", "properties": {
                    "codigo": { "type": "string" },
                    "naturaleza": { "type": "string", "enum": ["mano_obra", "material", "maquinaria", "subcontrata", "otros", "porcentaje"] },
                    "unidad": { "type": "string" },
                    "resumen": { "type": "string" },
                    "precio": num,
                    "pvp": num,
                    "fabricante": { "type": "string" },
                    "grupo": { "type": "string" }
                }, "required": ["codigo"] } },
                "descuentos": { "type": "array", "items": { "type": "object", "properties": { "fabricante": { "type": "string" }, "porcentaje": num }, "required": ["fabricante", "porcentaje"] } },
                "medios_auxiliares": num,
                "confianza": { "type": "string", "enum": ["alta", "media", "baja"] },
                "justificacion": { "type": "string" },
                "propuestas": { "type": "array", "items": { "type": "object", "properties": {
                    "partida": { "type": "string" },
                    "lineas": { "type": "array", "items": {} },
                    "confianza": { "type": "string", "enum": ["alta", "media", "baja"] },
                    "justificacion": { "type": "string" },
                    "nueva": { "type": "object", "description": "Partida nueva (contradictorio): como en proponer" }
                }, "required": ["partida", "lineas"] } }
            }, "required": ["propuestas"] }
        },
        {
            "name": "proponer_cambios",
            "description": "Propone cambios de la obra activa que no son de precio: quitar partidas o capítulos que no proceden (tipo borrar), cambiar la cantidad de una partida en su capítulo, opcionalmente con líneas de medición (tipo cantidad), rehacer el resumen, el texto descriptivo o la unidad de una partida (tipo texto) y moverla a otro capítulo (tipo mover). NO modifica el presupuesto: queda pendiente hasta que el usuario la acepte en el programa, y se aplican todos los cambios o ninguno. «capitulo» es el capítulo donde está la línea; puede omitirse si la partida solo está en un sitio. Para cambiar el descompuesto o el precio use proponer.",
            "inputSchema": { "type": "object", "properties": {
                "cambios": { "type": "array", "items": { "type": "object", "properties": {
                    "tipo": { "type": "string", "enum": ["borrar", "cantidad", "texto", "mover"] },
                    "partida": { "type": "string", "description": "Código de la partida (o del capítulo, al borrar o mover)" },
                    "capitulo": { "type": "string", "description": "Capítulo donde está ahora (borrar, cantidad, mover)" },
                    "cantidad": num,
                    "medicion": { "type": "array", "description": "Líneas de medición (cantidad = suma de uds × largo × ancho × alto de los campos rellenos)", "items": { "type": "object", "properties": {
                        "comentario": { "type": "string" },
                        "uds": num, "largo": num, "ancho": num, "alto": num
                    } } },
                    "resumen": { "type": "string" },
                    "texto": { "type": "string" },
                    "unidad": { "type": "string" },
                    "a": { "type": "string", "description": "Capítulo de destino (mover)" },
                    "antes_de": { "type": "string", "description": "Colocar delante de esta línea del destino (mover)" }
                }, "required": ["tipo", "partida"] } },
                "confianza": { "type": "string", "enum": ["alta", "media", "baja"] },
                "justificacion": { "type": "string", "description": "Por qué (documento, indicación del usuario...)" }
            }, "required": ["cambios", "justificacion"] }
        },
        {
            "name": "buscar_biblioteca",
            "description": "CONSULTAR ANTES DE ESTIMAR. Busca en la biblioteca de precios del usuario: partidas guardadas de obras anteriores (con su descompuesto y el precio de hoy de cada línea según la tarifa vigente y el descuento del fabricante), y líneas de tarifas (PVP) y ofertas (netos) de fabricantes. Avisa de tarifas de años anteriores.",
            "inputSchema": { "type": "object", "properties": {
                "texto": { "type": "string", "description": "Palabras a buscar (sin tildes ni mayúsculas), p. ej. «tubo ppr 32» o «rejilla koolair»" },
                "limite": { "type": "integer" }
            }, "required": ["texto"] }
        },
        {
            "name": "estado_biblioteca",
            "description": "Resumen de la biblioteca: nº de partidas y, por fabricante, tarifa vigente (y si es de un año anterior), descuentos, ofertas y subida estimada.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "guardar_en_biblioteca",
            "description": "Guarda en la biblioteca las partidas descompuestas con precio de la obra activa (o solo las indicadas) y aprende sus descuentos por fabricante. Úsela solo cuando el usuario lo pida. No duplica partidas sin cambios; si cambian, guarda una versión nueva.",
            "inputSchema": { "type": "object", "properties": {
                "origen": { "type": "string", "enum": ["real", "revisado", "estimado"], "description": "real: obra cerrada u ofertas reales; revisado: revisado por el usuario; estimado" },
                "partidas": { "type": "array", "items": { "type": "string" } }
            } }
        },
        {
            "name": "registrar_tarifa",
            "description": "Guarda en la biblioteca la tarifa (PVP por referencia) de un fabricante para un año, p. ej. leída de un adjunto. Las tarifas anteriores se conservan; se usa la más reciente y el descuento del fabricante no cambia. Devuelve la comparación con la tarifa anterior.",
            "inputSchema": { "type": "object", "properties": {
                "fabricante": { "type": "string" },
                "etiqueta": { "type": "string", "description": "p. ej. «2027» o «2027-1»" },
                "anio": { "type": "integer" },
                "lineas": { "type": "array", "items": { "type": "object", "properties": {
                    "referencia": { "type": "string" }, "descripcion": { "type": "string" }, "unidad": { "type": "string" },
                    "pvp": num, "grupo": { "type": "string" }
                }, "required": ["referencia", "pvp"] } }
            }, "required": ["fabricante", "etiqueta", "lineas"] }
        },
        {
            "name": "registrar_oferta",
            "description": "Guarda en la biblioteca una oferta en NETOS de un fabricante para una obra (p. ej. leída de un adjunto). Si hay tarifa del fabricante, calcula el descuento real por grupo. Los netos de la oferta sirven de referencia para otras obras (confianza media).",
            "inputSchema": { "type": "object", "properties": {
                "fabricante": { "type": "string" },
                "numero": { "type": "string" },
                "fecha": { "type": "string", "description": "AAAA-MM-DD" },
                "validez": { "type": "string", "description": "AAAA-MM-DD" },
                "obra": { "type": "string" },
                "documento": { "type": "string", "description": "Ruta del documento de la oferta (la del adjunto, tal cual)" },
                "lineas": { "type": "array", "items": { "type": "object", "properties": {
                    "referencia": { "type": "string" }, "descripcion": { "type": "string" }, "unidad": { "type": "string" },
                    "neto": num, "cantidad": num
                }, "required": ["referencia", "neto"] } }
            }, "required": ["fabricante", "lineas"] }
        },
        {
            "name": "fijar_descuento_biblioteca",
            "description": "Guarda en la biblioteca el descuento del instalador de un fabricante (o «FAB/GRUPO») y/o la subida estimada sobre su última tarifa mientras no llegue la nueva. El porcentaje puede ser simple (45) o encadenado como texto, hasta tres escalones (\"40+8\", \"50+10+5\"). Solo con datos que dé el usuario.",
            "inputSchema": { "type": "object", "properties": {
                "fabricante": { "type": "string" }, "porcentaje": num, "subida_estimada": num
            }, "required": ["fabricante"] }
        },
        {
            "name": "ver_propuestas",
            "description": "Propuestas hechas para la obra activa: autor (Claude, ChatGPT…), estado (pendiente, aceptada o rechazada por el usuario), comentarios de otros agentes y desacuerdos (partidas con propuestas distintas de varios agentes).",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "comentar_propuesta",
            "description": "Opinar sobre una propuesta (normalmente de otro agente): de acuerdo, en contra o solo un comentario, con el motivo (precio de tarifa, mano de obra que falta, medición…). No cambia la obra; el usuario lo ve junto a la propuesta.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Número de la propuesta (ver_propuestas)" },
                    "texto": { "type": "string", "description": "Motivo, concreto y con cifras si las hay" },
                    "de_acuerdo": { "type": "boolean", "description": "true de acuerdo, false en contra; sin indicar, solo comenta" }
                },
                "required": ["id", "texto"]
            }
        },
        {
            "name": "leer_mensajes",
            "description": "Encargos que el usuario ha dejado en el panel del asistente del programa para la obra activa (mensajes pendientes, con rutas de adjuntos como ofertas de fabricantes) y la conversación anterior.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "responder",
            "description": "Contesta en el panel del asistente del programa a los mensajes pendientes de la obra activa (los marca como atendidos). Resuma lo hecho: propuestas creadas, dudas, informes guardados.",
            "inputSchema": { "type": "object", "properties": {
                "texto": { "type": "string", "description": "Respuesta (Markdown sencillo)" },
                "mensajes": { "type": "array", "items": { "type": "integer" }, "description": "Números de los mensajes a los que responde; por defecto, todos los pendientes" }
            }, "required": ["texto"] }
        },
        {
            "name": "leer_adjunto",
            "description": "Lee un fichero adjunto del usuario (oferta, tarifa, mediciones): Excel/ODS como CSV por hoja, PDF como texto, CSV o texto plano.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string" } }, "required": ["ruta"] }
        },
        {
            "name": "guardar_informe",
            "description": "Guarda un informe en Markdown para la obra activa (resumen de valoración, comparativa de ofertas, dudas al cliente, memoria de legalización…). El usuario lo abre desde el panel del asistente.",
            "inputSchema": { "type": "object", "properties": {
                "titulo": { "type": "string" },
                "contenido": { "type": "string", "description": "Markdown" }
            }, "required": ["titulo", "contenido"] }
        },
        {
            "name": "exportar_excel",
            "description": "Genera el informe Excel (resumen, presupuesto, descompuestos, mediciones, recursos y horas por oficio) en un fichero nuevo.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string", "description": "Ruta del .xlsx a crear" } }, "required": ["ruta"] }
        },
        {
            "name": "exportar_bc3",
            "description": "Exporta el presupuesto a BC3 (FIEBDC-3/2002, ANSI, como Presto 8.8) en un fichero nuevo.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string", "description": "Ruta del .bc3 a crear" }, "longitud_codigo": { "type": ["integer", "string"], "description": "Para un Presto antiguo que corta los códigos al importar: longitud máxima (p. ej. 12). Los códigos más largos se renombran." } }, "required": ["ruta"] }
        }
    ])
}

/// Herramientas en el formato de la API de mensajes de Anthropic
/// (`name`, `description`, `input_schema`), sin las indicadas en `excluir`.
/// Las usa el modo directo del asistente del programa.
pub fn herramientas_api(excluir: &[&str]) -> Vec<Value> {
    herramientas()
        .as_array()
        .into_iter()
        .flatten()
        .filter(|h| !excluir.contains(&h["name"].as_str().unwrap_or("")))
        .map(|h| {
            json!({
                "name": h["name"],
                "description": h["description"],
                "input_schema": h["inputSchema"],
            })
        })
        .collect()
}

/// Ejecuta una herramienta (igual que `tools/call`). `Err` lleva el mensaje
/// de error para la IA.
pub fn ejecutar(estado: &mut Estado, nombre: &str, args: &Value) -> Result<String, String> {
    llamar(estado, nombre, args)
}

fn llamar(estado: &mut Estado, nombre: &str, a: &Value) -> Result<String, String> {
    match nombre {
        "abrir_presupuesto" => estado.abrir(texto(a, "ruta")?),
        "abrir_ejemplo" => {
            estado.presupuesto = Some(ppto_core::ejemplos::suelo_radiante());
            estado.origen = "ejemplo".into();
            estado.obra_activa = None;
            Ok("Abierto el ejemplo sintético de suelo radiante (PEM 7.005,15 €).".into())
        }
        "resumen" => resumen(estado),
        "buscar" => buscar(
            estado.p()?,
            texto(a, "texto")?,
            a.get("limite").and_then(Value::as_u64).unwrap_or(30),
        ),
        "ver_concepto" => ver_concepto(estado.p()?, texto(a, "codigo")?),
        "recursos" => recursos(estado.p()?, a.get("naturaleza").and_then(Value::as_str)),
        "simular_cambios" => simular_cambios(estado.p()?, a),
        "simular_subcontrata" => simular_subcontrata(estado.p()?, a),
        "abrir_obra_activa" => abrir_obra_activa(estado),
        "comprobar_recursos" => biblioteca::comprobar_recursos(estado),
        "comparar_obras" => biblioteca::comparar_obras(estado),
        "guardar_precios" => biblioteca::guardar_precios(estado, a),
        "ver_datos_obra" => documentos::ver_datos_obra(estado),
        "documentos_obra" => documentos::documentos_obra(estado),
        "leer_documento" => documentos::leer_documento(estado, a),
        "proponer_datos_obra" => documentos::proponer_datos_obra(estado, a),
        "leer_plano" => documentos::leer_plano(estado, a),
        "partidas_sin_precio" => {
            partidas_sin_precio(estado.p()?, a.get("limite").and_then(Value::as_u64).unwrap_or(40))
        }
        "buscar_tarifas" => buscar_tarifas(estado, texto(a, "texto")?),
        "proponer" => proponer(estado, a),
        "proponer_lote" => proponer_lote(estado, a),
        "proponer_cambios" => proponer_cambios(estado, a),
        "buscar_biblioteca" => biblioteca::buscar(estado, a),
        "estado_biblioteca" => biblioteca::estado_biblioteca(estado),
        "registrar_tarifa" => biblioteca::registrar_tarifa(estado, a),
        "registrar_oferta" => biblioteca::registrar_oferta(estado, a),
        "fijar_descuento_biblioteca" => biblioteca::fijar_descuento(estado, a),
        "guardar_en_biblioteca" => biblioteca::guardar_obra(estado, a),
        "ver_propuestas" => ver_propuestas(estado),
        "comentar_propuesta" => comentar_propuesta(estado, a),
        "leer_mensajes" => asistente::leer_mensajes(estado),
        "responder" => asistente::responder(estado, a),
        "leer_adjunto" => asistente::leer_adjunto(a),
        "guardar_informe" => asistente::guardar_informe(estado, a),
        "exportar_excel" => {
            let ruta = destino(estado, texto(a, "ruta")?, "xlsx")?;
            let o = ppto_informes::OpcionesInforme {
                gastos_generales: estado.gg,
                beneficio_industrial: estado.bi,
                iva: estado.iva,
            };
            ppto_informes::guardar_excel(estado.p()?, &o, &ruta).map_err(|e| e.to_string())?;
            Ok(format!("Informe Excel creado: {ruta}"))
        }
        "exportar_bc3" => {
            let ruta = destino(estado, texto(a, "ruta")?, "bc3")?;
            let o = ppto_bc3::OpcionesExportacion {
                gastos_generales: Some(estado.gg),
                beneficio_industrial: Some(estado.bi),
                iva: Some(estado.iva),
                fecha: None,
                descuentos_visibles: true,
                longitud_codigo: a.get("longitud_codigo").and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|t| t.trim().parse().ok()))
                        .map(|n| n as usize)
                }),
            };
            let avisos = ppto_bc3::exportar_fichero(estado.p()?, &o, &ruta)?;
            let mut t = format!("BC3 creado: {ruta}");
            for av in avisos {
                let _ = write!(t, "\n- aviso: {av}");
            }
            Ok(t)
        }
        otro => Err(format!("herramienta desconocida: {otro}")),
    }
}

/// Ruta de salida: con la extensión pedida y nunca el fichero de origen.
fn destino(estado: &Estado, ruta: &str, ext: &str) -> Result<String, String> {
    let ruta = ruta.trim().trim_matches('"');
    if !ruta.to_lowercase().ends_with(&format!(".{ext}")) {
        return Err(format!("la ruta debe terminar en .{ext}"));
    }
    let norm = |s: &str| s.replace('\\', "/").to_lowercase();
    if !estado.origen.is_empty() && norm(ruta) == norm(&estado.origen) {
        return Err("no se sobrescribe el fichero de origen: elija otro nombre".into());
    }
    Ok(ruta.to_owned())
}

pub(crate) fn texto<'a>(a: &'a Value, campo: &str) -> Result<&'a str, String> {
    a.get(campo)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("falta el parámetro «{campo}»"))
}

/// Número exacto a partir de JSON (número o texto con coma o punto), sin pasar por coma flotante.
fn decimal(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => {
            let s = n.to_string();
            Decimal::from_str(&s).or_else(|_| Decimal::from_scientific(&s)).ok()
        }
        Value::String(s) => {
            // Texto de una IA: «7.518» es 7,518 (punto decimal, como en JSON),
            // no 7.518 con separador de miles. Con coma o varios puntos, formato español.
            let t = s.trim();
            if !t.contains(',') && t.matches('.').count() == 1 {
                Decimal::from_str(t).ok()
            } else {
                ppto_core::formato::leer(t)
            }
        }
        _ => None,
    }
}

pub(crate) fn dec_campo(a: &Value, campo: &str) -> Result<Option<Decimal>, String> {
    match a.get(campo) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => decimal(v)
            .map(Some)
            .ok_or_else(|| format!("«{campo}» no es un número válido")),
    }
}

/// Descuento simple (45 o "45") o encadenado ("40+8", hasta tres escalones).
pub(crate) fn cadena_campo(a: &Value, campo: &str) -> Result<Option<Vec<Decimal>>, String> {
    match a.get(campo) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => ppto_core::descuentos::leer_cadena(s)
            .map(Some)
            .map_err(|e| format!("«{campo}»: {e}")),
        Some(v) => decimal(v)
            .map(|d| Some(vec![d]))
            .ok_or_else(|| format!("«{campo}» no es un número válido")),
    }
}

fn resumen(estado: &Estado) -> Result<String, String> {
    let p = estado.p()?;
    let v = p.valorar().map_err(|e| e.to_string())?;
    let mut t = format!(
        "# {}\n\n| Capítulo | Resumen | Importe € | % |\n|---|---|---:|---:|\n",
        p.nombre
    );
    for l in v.lineas.get(&p.raiz).into_iter().flatten() {
        let c = &p.conceptos[&l.hijo];
        let pct = if v.pem.is_zero() {
            Decimal::ZERO
        } else {
            l.importe / v.pem * Decimal::ONE_HUNDRED
        };
        let _ = writeln!(
            t,
            "| {} | {} | {} | {} |",
            c.codigo,
            c.resumen,
            eur(l.importe),
            num(pct, 2)
        );
    }
    let r = venta::resumen(v.pem, estado.gg, estado.bi, estado.iva, p.decimales.importe);
    t.push('\n');
    if !p.costes_indirectos.is_zero() {
        let _ = writeln!(t, "- Coste directo: {} €", eur(v.coste_directo));
        let _ = writeln!(
            t,
            "- Costes indirectos ({} % por partida): {} €",
            num(p.costes_indirectos, 2),
            eur(v.pem - v.coste_directo)
        );
    }
    let _ = writeln!(t, "- **PEM: {} €**", eur(r.pem));
    let _ = writeln!(t, "- GG {} %: {} €", num(estado.gg, 2), eur(r.gastos_generales));
    let _ = writeln!(t, "- BI {} %: {} €", num(estado.bi, 2), eur(r.beneficio_industrial));
    let _ = writeln!(t, "- Base: {} €", eur(r.base));
    let _ = writeln!(t, "- IVA {} %: {} €", num(estado.iva, 2), eur(r.iva));
    let _ = writeln!(t, "- **Total: {} €**", eur(r.total));
    Ok(t)
}

fn plegar(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

fn buscar(p: &Presupuesto, consulta: &str, limite: u64) -> Result<String, String> {
    let v = p.valorar().map_err(|e| e.to_string())?;
    let palabras: Vec<String> = plegar(consulta).split_whitespace().map(str::to_owned).collect();
    let mut t = String::from("| Código | Ud | Resumen | Naturaleza | Precio € |\n|---|---|---|---|---:|\n");
    let mut n = 0u64;
    let mut total = 0u64;
    for c in p.conceptos.values() {
        let heno = plegar(&format!(
            "{} {} {}",
            c.codigo,
            c.resumen,
            c.texto.as_deref().unwrap_or("")
        ));
        if palabras.iter().all(|w| heno.contains(w.as_str())) {
            total += 1;
            if n < limite {
                n += 1;
                let precio = v.precios.get(&c.codigo).copied().unwrap_or_default();
                let _ = writeln!(
                    t,
                    "| {} | {} | {} | {} | {} |",
                    c.codigo,
                    c.unidad,
                    c.resumen,
                    naturaleza(c.naturaleza),
                    eur(precio)
                );
            }
        }
    }
    if total == 0 {
        return Ok(format!("Sin resultados para «{consulta}»."));
    }
    if total > n {
        let _ = write!(t, "\n{total} resultados; se muestran {n}.");
    }
    Ok(t)
}

fn ver_concepto(p: &Presupuesto, codigo: &str) -> Result<String, String> {
    let c = p.concepto(codigo.trim()).map_err(|e| e.to_string())?;
    let v = p.valorar().map_err(|e| e.to_string())?;
    let coste = v.precios.get(&c.codigo).copied().unwrap_or_default();
    let mut t = format!(
        "# {} · {} · {}\n\nNaturaleza: {}\n",
        c.codigo,
        c.unidad,
        c.resumen,
        naturaleza(c.naturaleza)
    );
    if let Some(tf) = &c.tarifa {
        let _ = writeln!(
            t,
            "Tarifa {}: PVP {} € − {} % = {} € neto",
            p.clave_aplicada(tf),
            eur(tf.pvp),
            num(p.descuento_tarifa(tf), 2),
            eur(c.precio)
        );
    }
    if c.naturaleza == Naturaleza::Capitulo {
        let _ = writeln!(t, "Importe del capítulo: {} €", eur(coste));
    } else {
        let _ = writeln!(t, "Coste directo unitario: {} €/{}", eur(coste), c.unidad);
    }
    if let Some(texto) = &c.texto {
        let _ = writeln!(t, "\nTexto descriptivo: {texto}");
    }
    // Apariciones en capítulos
    let mut aparece = String::new();
    for (padre, lineas) in &v.lineas {
        if p.conceptos[padre].naturaleza != Naturaleza::Capitulo {
            continue;
        }
        for l in lineas.iter().filter(|l| l.hijo == c.codigo) {
            let _ = writeln!(
                aparece,
                "| {padre} | {} | {} | {} |",
                num(l.cantidad, p.decimales.medicion),
                eur(l.precio),
                eur(l.importe)
            );
        }
    }
    if !aparece.is_empty() {
        let _ = write!(
            t,
            "\n## En capítulos (precio con indirectos)\n| Capítulo | Cantidad | Precio € | Importe € |\n|---|---:|---:|---:|\n{aparece}"
        );
    }
    if let Some(lineas) = v.lineas.get(&c.codigo).filter(|_| c.naturaleza != Naturaleza::Capitulo) {
        t.push_str("\n## Descompuesto\n| Código | Ud | Resumen | Cantidad | Precio € | Importe € |\n|---|---|---|---:|---:|---:|\n");
        for l in lineas {
            let h = &p.conceptos[&l.hijo];
            let precio = if h.naturaleza == Naturaleza::Porcentaje {
                format!("{} (base)", eur(l.precio))
            } else {
                eur(l.precio)
            };
            let _ = writeln!(
                t,
                "| {} | {} | {} | {} | {precio} | {} |",
                h.codigo,
                h.unidad,
                h.resumen,
                num(l.cantidad, p.decimales.rendimiento),
                eur(l.importe)
            );
        }
        let _ = writeln!(t, "| | | **Coste directo** | | | **{}** |", eur(coste));
    }
    for ((padre, hijo), m) in &p.mediciones {
        if hijo != &c.codigo {
            continue;
        }
        let r = m.calcular(&p.decimales, &c.unidad).map_err(|e| e.to_string())?;
        let _ = write!(
            t,
            "\n## Medición en {padre}\n| Comentario | Uds | Long | Anch | Alt | Parcial |\n|---|---:|---:|---:|---:|---:|\n"
        );
        let s = |x: Option<Decimal>| x.map(|d| num(d, p.decimales.dimensiones)).unwrap_or_default();
        for (i, l) in m.lineas.iter().enumerate() {
            let com = l
                .formula
                .as_ref()
                .map_or(l.comentario.clone(), |f| format!("{} [{f}]", l.comentario));
            let _ = writeln!(
                t,
                "| {com} | {} | {} | {} | {} | {} |",
                s(l.unidades),
                s(l.longitud),
                s(l.anchura),
                s(l.altura),
                num(r.parciales.get(i).copied().unwrap_or_default(), p.decimales.medicion)
            );
        }
        let _ = writeln!(
            t,
            "| **Total** | | | | | **{} {}** |",
            num(r.total, p.decimales.medicion),
            c.unidad
        );
    }
    Ok(t)
}

fn recursos(p: &Presupuesto, filtro: Option<&str>) -> Result<String, String> {
    let e = p.explosion_recursos().map_err(|e| e.to_string())?;
    let elegir = |n: Naturaleza| match filtro {
        None => true,
        Some("mano_obra") => n == Naturaleza::ManoObra,
        Some("material") => n == Naturaleza::Material,
        Some("maquinaria") => n == Naturaleza::Maquinaria,
        Some("subcontrata") => n == Naturaleza::Subcontrata,
        Some("otros") => matches!(n, Naturaleza::Otros | Naturaleza::Porcentaje),
        Some(_) => true,
    };
    let mut t = String::from(
        "| Código | Ud | Resumen | Naturaleza | Cantidad | Precio € | Importe € |\n|---|---|---|---|---:|---:|---:|\n",
    );
    for r in e.recursos.iter().filter(|r| elegir(r.naturaleza)) {
        let _ = writeln!(
            t,
            "| {} | {} | {} | {} | {} | {} | {} |",
            r.codigo,
            r.unidad,
            r.resumen,
            naturaleza(r.naturaleza),
            num(r.cantidad, 3),
            eur(r.precio),
            eur(r.importe)
        );
    }
    let horas: Decimal = e.por_naturaleza(Naturaleza::ManoObra).map(|r| r.cantidad).sum();
    let mo: Decimal = e.por_naturaleza(Naturaleza::ManoObra).map(|r| r.importe).sum();
    let _ = write!(
        t,
        "\nHoras de mano de obra: {} · importe {} €\nTotal recursos {} € · coste directo {} € · descuadre de redondeo {} €",
        num(horas, 2),
        eur(mo),
        eur(e.total),
        eur(e.coste_directo),
        eur(e.descuadre_redondeo)
    );
    Ok(t)
}

fn simular_cambios(p: &Presupuesto, a: &Value) -> Result<String, String> {
    let antes = p.valorar().map_err(|e| e.to_string())?;
    let mut q = p.clone();
    let mut cambios = Vec::new();
    if let Some(ci) = dec_campo(a, "costes_indirectos")? {
        if ci < Decimal::ZERO {
            return Err("los costes indirectos no pueden ser negativos".into());
        }
        cambios.push(format!("CI {} % → {} %", num(q.costes_indirectos, 2), num(ci, 2)));
        q.costes_indirectos = ci;
    }
    for x in a.get("precios").and_then(Value::as_array).into_iter().flatten() {
        let codigo = texto(x, "codigo")?;
        let precio = dec_campo(x, "precio")?.ok_or("falta precio")?;
        let anterior = q.concepto(codigo).map_err(|e| e.to_string())?.precio;
        q.fijar_precio(codigo, precio).map_err(|e| e.to_string())?;
        cambios.push(format!("{codigo}: {} → {} €", eur(anterior), eur(precio)));
    }
    for x in a.get("cantidades").and_then(Value::as_array).into_iter().flatten() {
        let (cap, partida) = (texto(x, "capitulo")?, texto(x, "partida")?);
        let cantidad = dec_campo(x, "cantidad")?.ok_or("falta cantidad")?;
        q.fijar_rendimiento(cap, partida, cantidad).map_err(|e| e.to_string())?;
        cambios.push(format!("{partida} en {cap}: cantidad {}", num(cantidad, 2)));
    }
    if cambios.is_empty() {
        return Err("no se ha indicado ningún cambio".into());
    }
    let despues = q.valorar().map_err(|e| e.to_string())?;
    let mut t = String::from("Simulación (el presupuesto abierto no se modifica)\n\nCambios:\n");
    for c in &cambios {
        let _ = writeln!(t, "- {c}");
    }
    let dif = despues.pem - antes.pem;
    let pct = if antes.pem.is_zero() {
        Decimal::ZERO
    } else {
        dif / antes.pem * Decimal::ONE_HUNDRED
    };
    let _ = write!(
        t,
        "\n| | Antes | Después | Diferencia |\n|---|---:|---:|---:|\n| Coste directo | {} | {} | {} |\n| PEM | {} | {} | {} ({} %) |\n",
        eur(antes.coste_directo),
        eur(despues.coste_directo),
        eur(despues.coste_directo - antes.coste_directo),
        eur(antes.pem),
        eur(despues.pem),
        eur(dif),
        num(pct, 2)
    );
    // Partidas cuyo importe en capítulo cambia más
    let mut deltas: Vec<(String, Decimal)> = Vec::new();
    for (padre, lineas) in &despues.lineas {
        if q.conceptos[padre].naturaleza != Naturaleza::Capitulo {
            continue;
        }
        for l in lineas
            .iter()
            .filter(|l| q.conceptos[&l.hijo].naturaleza != Naturaleza::Capitulo)
        {
            let previo = antes
                .lineas
                .get(padre)
                .and_then(|v| v.iter().find(|x| x.hijo == l.hijo))
                .map(|x| x.importe)
                .unwrap_or_default();
            if l.importe != previo {
                deltas.push((format!("{} en {padre}", l.hijo), l.importe - previo));
            }
        }
    }
    deltas.sort_by_key(|d| std::cmp::Reverse(d.1.abs()));
    if !deltas.is_empty() {
        t.push_str("\nPartidas más afectadas:\n");
        for (d, v) in deltas.iter().take(10) {
            let _ = writeln!(t, "- {d}: {} €", eur(*v));
        }
    }
    Ok(t)
}

fn simular_subcontrata(p: &Presupuesto, a: &Value) -> Result<String, String> {
    let partida = texto(a, "partida")?;
    let c = p.concepto(partida).map_err(|e| e.to_string())?;
    let recursos: Vec<String> = match a.get("recursos").and_then(Value::as_array) {
        Some(v) if !v.is_empty() => v.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ => c
            .descomposicion
            .iter()
            .filter(|l| {
                p.conceptos
                    .get(&l.hijo)
                    .is_some_and(|h| h.naturaleza == Naturaleza::ManoObra)
            })
            .map(|l| l.hijo.clone())
            .collect(),
    };
    if recursos.is_empty() {
        return Err(format!(
            "la partida {partida} no tiene mano de obra: indique «recursos»"
        ));
    }
    let (contratacion, factor) = match texto(a, "modalidad")? {
        "unidad" => {
            let precio = dec_campo(a, "precio")?.ok_or("falta «precio» por unidad de contratación")?;
            let unidad = a.get("unidad").and_then(Value::as_str).unwrap_or(&c.unidad).to_owned();
            let factor = dec_campo(a, "factor")?.unwrap_or(Decimal::ONE);
            (Contratacion::PorUnidad { unidad, precio }, factor)
        }
        "alzado" => {
            let importe = dec_campo(a, "importe")?.ok_or("falta «importe» del alzado")?;
            (Contratacion::Alzado { importe }, Decimal::ZERO)
        }
        otra => return Err(format!("modalidad «{otra}» no válida: unidad o alzado")),
    };
    let paquete = PaqueteTrabajo {
        codigo: "SIM".into(),
        descripcion: format!("Subcontrata de {partida}"),
        contratacion,
        asignaciones: vec![Asignacion {
            partida: partida.to_owned(),
            recursos: recursos.clone(),
            factor_conversion: factor,
        }],
        incluye: vec![],
        excluye: vec![],
    };
    let r = simular(p, std::slice::from_ref(&paquete)).map_err(|e| e.to_string())?;
    let eq = r.precio_equivalente("SIM", partida, 2).unwrap_or_default();
    let horas: Decimal = r.horas_liberadas.values().copied().sum();
    let mut t = format!(
        "Subcontrata de {partida} ({}) sustituyendo {} — el presupuesto no se modifica\n\n\
         | | Importe |\n|---|---:|\n\
         | Coste propio retirado | {} € |\n| Coste contratado | {} € |\n| Equivale a | {} €/{} de partida |\n\
         | Coste directo antes | {} € |\n| Coste directo con subcontrata | {} € |\n| **Ahorro** | **{} €** |\n\n\
         Horas propias liberadas: {}\n",
        c.resumen,
        recursos.join(", "),
        eur(r.coste_retirado),
        eur(r.coste_contratado),
        eur(eq),
        c.unidad,
        eur(r.coste_original),
        eur(r.coste_escenario),
        eur(r.ahorro),
        num(horas, 2)
    );
    for (k, h) in &r.horas_liberadas {
        let _ = writeln!(t, "- {k}: {} h", num(*h, 2));
    }
    for n in &r.notas {
        let _ = writeln!(t, "\nNota: {n}");
    }
    Ok(t)
}

fn naturaleza(n: Naturaleza) -> &'static str {
    match n {
        Naturaleza::Capitulo => "capítulo",
        Naturaleza::Partida => "partida",
        Naturaleza::ManoObra => "mano de obra",
        Naturaleza::Maquinaria => "maquinaria",
        Naturaleza::Material => "material",
        Naturaleza::Subcontrata => "subcontrata",
        Naturaleza::Porcentaje => "porcentaje",
        Naturaleza::Otros => "otros",
    }
}

// ---------------------------------------------------------------- propuestas (P-021)

pub(crate) fn abrir_obra_activa(estado: &mut Estado) -> Result<String, String> {
    let ruta = estado
        .ruta_obra_activa
        .clone()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let o = ppto_propuestas::ObraActiva::cargar(&ruta)?;
    if o.datos.contains_key(ppto_propuestas::DATO_EJEMPLO) {
        return Err(
            "La obra abierta en el programa es el EJEMPLO para practicar (datos inventados). \
                    Pida que abra la obra real (Archivo → Abrir o la pantalla de inicio) antes de valorar; \
                    no proponga precios sobre el ejemplo."
                .into(),
        );
    }
    let p = o.presupuesto;
    let partidas: Vec<&ppto_core::Concepto> = p
        .conceptos
        .values()
        .filter(|c| c.naturaleza == Naturaleza::Partida)
        .collect();
    if partidas.is_empty() && p.conceptos.len() <= 1 {
        return Err("El programa tiene abierto un presupuesto vacío: pida que abra la obra \
                    (Archivo → Abrir, una reciente de la pantalla de inicio o «Importar ciego del cliente»)."
            .into());
    }
    let v = p.valorar().map_err(|e| e.to_string())?;
    let sin = partidas
        .iter()
        .filter(|c| v.precios.get(&c.codigo).is_none_or(Decimal::is_zero))
        .count();
    let t = format!(
        "Obra activa del programa: {} — {} partidas, {} sin precio, PEM {} €.",
        o.clave,
        partidas.len(),
        sin,
        eur(v.pem)
    );
    let mut t = t;
    let fabs = p.fabricantes();
    if !fabs.is_empty() {
        t.push_str("\nDescuentos de la obra: ");
        t.push_str(
            &fabs
                .iter()
                .map(|f| format!("{} {} % ({} recursos)", f.nombre, num(f.descuento, 2), f.recursos))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    // Datos de la obra: si hay documentos y faltan datos, que se lean antes de valorar
    let es: Vec<ppto_core::especialidad::Especialidad> = o
        .datos
        .get("especialidad")
        .map(|e| {
            e.split(',')
                .filter_map(ppto_core::especialidad::Especialidad::leer)
                .collect()
        })
        .unwrap_or_default();
    let ficha = ppto_core::ficha::de_datos(&o.datos);
    let textos = ppto_core::ficha::textos_de_datos(&o.datos);
    let faltan = ppto_core::ficha::campos_de(&es)
        .iter()
        .filter(|c| !ficha.contains_key(c.clave))
        .count()
        + ppto_core::ficha::TEXTOS
            .iter()
            .filter(|c| c.clave != "documentos" && !textos.contains_key(c.clave))
            .count();
    match textos.get("documentos") {
        Some(d) if faltan > 0 => {
            let _ = write!(
                t,
                "\n\nLa obra tiene carpeta de documentos ({d}) y faltan {faltan} datos de la obra: ANTES de valorar, \
                 léalos (ver_datos_obra, documentos_obra, leer_documento de la memoria y el pliego) y propóngalos \
                 con proponer_datos_obra; use también los documentos para marcas, modelos y alcance."
            );
        }
        Some(d) => {
            let _ = write!(
                t,
                "\n\nCarpeta de documentos: {d}. Úsela (documentos_obra, leer_documento) para marcas, modelos y alcance."
            );
        }
        None if faltan > 0 => {
            let _ = write!(
                t,
                "\n\nFaltan {faltan} datos de la obra y no tiene carpeta de documentos: si el usuario tiene planos \
                 o memorias, que elija la carpeta en Obra → Datos de la obra."
            );
        }
        None => {}
    }
    estado.presupuesto = Some(p);
    estado.origen = String::new();
    estado.obra_activa = Some(o.clave);
    Ok(t)
}

fn partidas_sin_precio(p: &Presupuesto, limite: u64) -> Result<String, String> {
    let v = p.valorar().map_err(|e| e.to_string())?;
    let cant = p.cantidades_partidas().map_err(|e| e.to_string())?;
    // Capítulo de cada partida
    let mut capitulo = std::collections::BTreeMap::new();
    for (padre, c) in &p.conceptos {
        if c.naturaleza == Naturaleza::Capitulo {
            for l in &c.descomposicion {
                capitulo.entry(l.hijo.clone()).or_insert_with(|| padre.clone());
            }
        }
    }
    let mut t = String::new();
    let (mut n, mut total) = (0u64, 0u64);
    for (codigo, q) in &cant {
        let c = &p.conceptos[codigo];
        if c.fuera_de_importe() || !v.precios.get(codigo).is_none_or(Decimal::is_zero) {
            continue;
        }
        total += 1;
        if n >= limite {
            continue;
        }
        n += 1;
        let cap = capitulo.get(codigo).map(String::as_str).unwrap_or("");
        let _ = writeln!(
            t,
            "## {} · {} · {} {}\nCapítulo: {} {}",
            c.codigo,
            c.resumen,
            num(*q, p.decimales.medicion),
            c.unidad,
            cap,
            p.conceptos.get(cap).map(|x| x.resumen.as_str()).unwrap_or("")
        );
        if let Some(tx) = c.texto.as_deref().filter(|x| !x.trim().is_empty()) {
            let corto: String = tx.chars().take(600).collect();
            let _ = writeln!(t, "{corto}");
        }
        t.push('\n');
    }
    if total == 0 {
        return Ok("Todas las partidas tienen precio.".into());
    }
    let _ = write!(t, "{total} partidas sin precio; se muestran {n}.");
    Ok(t)
}

fn buscar_tarifas(estado: &Estado, consulta: &str) -> Result<String, String> {
    let ruta = estado
        .ruta_tarifas
        .as_ref()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let b = ppto_tarifas::Biblioteca::cargar(ruta);
    if b.tarifas.is_empty() {
        return Ok("El usuario no tiene tarifas de subcontratas cargadas.".into());
    }
    let e = b.buscar(consulta);
    if e.is_empty() {
        return Ok(format!("Sin resultados en las tarifas para «{consulta}»."));
    }
    let mut t =
        String::from("| Código | Ud | Descripción | Precio € | Tarifa | Incluye |\n|---|---|---|---:|---|---|\n");
    for x in e.iter().take(40) {
        if let Some((tf, l)) = b.linea(*x) {
            let _ = writeln!(
                t,
                "| {} | {} | {} | {} | {} | {} |",
                tf.codigo(l),
                l.unidad,
                l.descripcion,
                eur(l.precio),
                tf.nombre(),
                tf.notas
            );
        }
    }
    if e.len() > 40 {
        let _ = write!(t, "\n{} resultados; se muestran 40.", e.len());
    }
    Ok(t)
}

pub(crate) fn nombre_naturaleza(n: Naturaleza) -> &'static str {
    match n {
        Naturaleza::ManoObra => "mano_obra",
        Naturaleza::Material => "material",
        Naturaleza::Maquinaria => "maquinaria",
        Naturaleza::Subcontrata => "subcontrata",
        Naturaleza::Porcentaje => "porcentaje",
        Naturaleza::Partida => "partida",
        Naturaleza::Capitulo => "capitulo",
        _ => "otros",
    }
}

fn leer_naturaleza(s: &str) -> Option<Naturaleza> {
    Some(match plegar(s).replace([' ', '-'], "_").as_str() {
        "mano_obra" | "mano_de_obra" | "manoobra" => Naturaleza::ManoObra,
        "material" | "materiales" => Naturaleza::Material,
        "maquinaria" => Naturaleza::Maquinaria,
        "subcontrata" => Naturaleza::Subcontrata,
        "otros" => Naturaleza::Otros,
        "porcentaje" | "%" => Naturaleza::Porcentaje,
        _ => return None,
    })
}

fn proponer(estado: &mut Estado, a: &Value) -> Result<String, String> {
    use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta, LineaPropuesta, Propuesta};
    let obra = estado
        .obra_activa
        .clone()
        .ok_or("las propuestas van a la obra abierta en el programa: use antes abrir_obra_activa")?;
    let p = estado.p()?;
    let partida = texto(a, "partida")?.trim().to_owned();
    let confianza = Confianza::leer(texto(a, "confianza")?).ok_or("confianza: alta, media o baja")?;
    let justificacion = texto(a, "justificacion")?.trim().to_owned();
    let contenido = match (a.get("lineas").and_then(Value::as_array), dec_campo(a, "precio")?) {
        (_, Some(_)) => {
            return Err(
                "no se admiten precios directos: proponga siempre un descompuesto (lineas) con material, \
                 mano de obra, maquinaria o subcontrata y medios auxiliares"
                    .into(),
            );
        }
        (Some(ls), None) => {
            let mut v = Vec::new();
            for l in ls {
                let codigo = texto(l, "codigo")?.trim().to_owned();
                let rendimiento = dec_campo(l, "rendimiento")?.ok_or_else(|| format!("«{codigo}» sin rendimiento"))?;
                let naturaleza = match l.get("naturaleza").and_then(Value::as_str) {
                    None => None,
                    Some(n) => Some(leer_naturaleza(n).ok_or_else(|| format!("naturaleza no válida «{n}»"))?),
                };
                v.push(LineaPropuesta {
                    codigo,
                    rendimiento,
                    naturaleza,
                    unidad: l.get("unidad").and_then(Value::as_str).unwrap_or("").to_owned(),
                    resumen: l.get("resumen").and_then(Value::as_str).unwrap_or("").to_owned(),
                    precio: dec_campo(l, "precio")?,
                    pvp: dec_campo(l, "pvp")?,
                    fabricante: l
                        .get("fabricante")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .map(str::to_owned),
                    grupo: l
                        .get("grupo")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|g| !g.is_empty())
                        .map(str::to_owned),
                });
            }
            Contenido::Descompuesto(v)
        }
        (None, None) => return Err("indique lineas (el descompuesto de la partida)".into()),
    };
    let mut prop = Propuesta {
        id: 0,
        partida: partida.clone(),
        contenido,
        confianza,
        justificacion,
        autor: estado.autor.clone(),
        estado: EstadoPropuesta::Pendiente,
        descuentos: leer_descuentos(a)?,
        nueva: leer_nueva(a)?,
    };
    let sim = p
        .simular_propuesta(&prop)
        .map_err(|e| format!("propuesta no válida: {e}"))?;
    let ruta = estado
        .ruta_bandeja
        .clone()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let id = ppto_propuestas::Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop.clone()))
        .map_err(|e| format!("no se pudo guardar la propuesta: {e}"))?;
    prop.id = id;
    let mut t = format!(
        "Propuesta #{id} para {partida}: {} → {} € (pendiente de que el usuario la acepte en el programa).",
        eur(sim.precio_antes),
        eur(sim.precio_despues)
    );
    if !sim.nuevos.is_empty() {
        let _ = write!(t, "\nRecursos nuevos: {}.", sim.nuevos.join(", "));
    }
    for (c, antes, despues) in &sim.ligados {
        let _ = write!(
            t,
            "\n{c} se liga a la tarifa del fabricante: neto {} → {} €.",
            eur(*antes),
            eur(*despues)
        );
    }
    for (f, pct) in &sim.descuentos_nuevos {
        let _ = write!(
            t,
            "\nDescuento {f} {} % (estimado; la obra no lo tenía: el usuario lo puede cambiar para todos sus recursos).",
            num(*pct, 2)
        );
    }
    for (c, obra, propuesto) in &sim.precios_distintos {
        let _ = write!(
            t,
            "\nAviso: {c} está en la obra a {} € (usted indicó {} €): se usará el de la obra.",
            eur(*obra),
            eur(*propuesto)
        );
    }
    if let Contenido::Descompuesto(lineas) = &prop.contenido {
        let netos = materiales_a_neto(p, lineas);
        if !netos.is_empty() {
            let _ = write!(
                t,
                "\n{AVISO_NETO}{}. El usuario quiere ver siempre PVP − descuento: \
                 si son de catálogo de un fabricante o distribuidor, vuelva a proponer con pvp, fabricante (y grupo) \
                 y el descuento (si solo conoce el neto y el descuento, pvp = neto / (1 − descuento)). \
                 Los estimados llevan «(estimado)» en el resumen.",
                netos.join(", ")
            );
        }
    }
    Ok(t)
}

/// Inicio del aviso de materiales a neto (el lote lo reconoce y los junta).
const AVISO_NETO: &str = "Aviso: materiales a precio neto, sin tarifa: ";

/// Materiales de la propuesta que quedarían a precio neto sin tarifa de
/// fabricante: nuevos sin PVP, o ya en la obra sin tarifa y sin PVP en la
/// propuesta. No cuenta los marcados como estimados en el resumen.
fn materiales_a_neto(p: &Presupuesto, lineas: &[ppto_core::propuesta::LineaPropuesta]) -> Vec<String> {
    let estimado = |r: &str| r.to_lowercase().contains("estimad");
    lineas
        .iter()
        .filter(|l| l.pvp.is_none())
        .filter(|l| match p.conceptos.get(&l.codigo) {
            Some(c) => c.naturaleza == Naturaleza::Material && c.tarifa.is_none() && !estimado(&c.resumen),
            None => l.naturaleza == Some(Naturaleza::Material) && !estimado(&l.resumen),
        })
        .map(|l| l.codigo.clone())
        .collect()
}

/// Máximo de cambios por propuesta.
const MAX_CAMBIOS: usize = 60;

/// Cambios de la obra (quitar, cantidad/medición, textos, mover) en una
/// propuesta pendiente de aceptar.
fn proponer_cambios(estado: &mut Estado, a: &Value) -> Result<String, String> {
    use ppto_core::propuesta::{Cambio, Confianza, Contenido, EstadoPropuesta, Propuesta};
    use ppto_core::{LineaMedicion, Medicion};
    let obra = estado
        .obra_activa
        .clone()
        .ok_or("las propuestas van a la obra abierta en el programa: use antes abrir_obra_activa")?;
    let p = estado.p()?;
    let lista = a
        .get("cambios")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
        .ok_or("indique cambios (lista de {tipo, partida, ...})")?;
    if lista.len() > MAX_CAMBIOS {
        return Err(format!("como mucho {MAX_CAMBIOS} cambios por propuesta"));
    }
    let confianza = match a.get("confianza").and_then(Value::as_str) {
        None => Confianza::Media,
        Some(c) => Confianza::leer(c).ok_or("confianza: alta, media o baja")?,
    };
    let justificacion = texto(a, "justificacion")?.trim().to_owned();
    let opt = |c: &Value, k: &str| c.get(k).and_then(Value::as_str).map(|s| s.trim().to_owned());
    let mut cambios = Vec::new();
    for (i, c) in lista.iter().enumerate() {
        let n = i + 1;
        let partida = texto(c, "partida")
            .map_err(|e| format!("cambio {n}: {e}"))?
            .trim()
            .to_owned();
        let capitulo = opt(c, "capitulo").unwrap_or_default();
        let tipo = texto(c, "tipo").map_err(|e| format!("cambio {n}: {e}"))?;
        let cambio = match tipo.trim().to_lowercase().as_str() {
            "borrar" | "quitar" => Cambio::Borrar { capitulo, partida },
            "cantidad" | "medicion" => {
                let mut medicion = Vec::new();
                if let Some(ls) = c.get("medicion").and_then(Value::as_array) {
                    for l in ls {
                        medicion.push(LineaMedicion::normal(
                            l.get("comentario").and_then(Value::as_str).unwrap_or(""),
                            dec_campo(l, "uds")?,
                            dec_campo(l, "largo")?,
                            dec_campo(l, "ancho")?,
                            dec_campo(l, "alto")?,
                        ));
                    }
                }
                let cantidad = match dec_campo(c, "cantidad")? {
                    Some(q) => q,
                    None if !medicion.is_empty() => {
                        let unidad = p
                            .concepto(&partida)
                            .map_err(|e| format!("cambio {n}: {e}"))?
                            .unidad
                            .clone();
                        Medicion::new(medicion.clone())
                            .calcular(&p.decimales, &unidad)
                            .map_err(|e| format!("cambio {n}: {e}"))?
                            .total
                    }
                    None => return Err(format!("cambio {n} ({partida}): indique cantidad o medicion")),
                };
                Cambio::Cantidad {
                    capitulo,
                    partida,
                    cantidad,
                    medicion,
                }
            }
            "texto" => Cambio::Texto {
                partida,
                resumen: opt(c, "resumen"),
                texto: c.get("texto").and_then(Value::as_str).map(str::to_owned),
                unidad: opt(c, "unidad"),
            },
            "mover" => Cambio::Mover {
                de: capitulo,
                partida,
                a: opt(c, "a")
                    .filter(|x| !x.is_empty())
                    .ok_or_else(|| format!("cambio {n}: indique el capítulo de destino «a»"))?,
                antes_de: opt(c, "antes_de").filter(|x| !x.is_empty()),
            },
            otro => {
                return Err(format!(
                    "cambio {n}: tipo «{otro}» no válido (borrar, cantidad, texto o mover)"
                ));
            }
        };
        cambios.push(cambio);
    }
    let mut codigos: Vec<&str> = cambios.iter().map(Cambio::partida).collect();
    codigos.dedup();
    let mut clave = format!("cambios: {}", codigos.join(", "));
    if clave.chars().count() > 80 {
        clave = format!("{}…", clave.chars().take(79).collect::<String>());
    }
    let prop = Propuesta {
        id: 0,
        partida: clave,
        contenido: Contenido::Cambios(cambios),
        confianza,
        justificacion,
        autor: estado.autor.clone(),
        estado: EstadoPropuesta::Pendiente,
        descuentos: Default::default(),
        nueva: None,
    };
    let sim = p
        .simular_propuesta(&prop)
        .map_err(|e| format!("propuesta no válida (no se ha guardado nada): {e}"))?;
    let ruta = estado
        .ruta_bandeja
        .clone()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let id = ppto_propuestas::Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop.clone()))
        .map_err(|e| format!("no se pudo guardar la propuesta: {e}"))?;
    let mut t = format!(
        "Propuesta #{id} con {} cambios (pendiente de que el usuario la acepte en el programa). PEM {} → {} €.",
        sim.cambios.len(),
        eur(sim.pem_antes),
        eur(sim.pem_despues)
    );
    for c in &sim.cambios {
        let _ = write!(t, "\n- {c}");
    }
    Ok(t)
}

/// Partida nueva (contradictorio): `nueva: {capitulo, capitulo_resumen,
/// despues_de, unidad, resumen, texto, cantidad}`.
fn leer_nueva(a: &Value) -> Result<Option<ppto_core::propuesta::NuevaPartida>, String> {
    let Some(n) = a.get("nueva").filter(|n| !n.is_null()) else {
        return Ok(None);
    };
    // Algunos clientes con el esquema antiguo envían el objeto como texto JSON.
    let leido;
    let n = match n.as_str() {
        Some(t) => {
            leido = serde_json::from_str::<Value>(t).map_err(|e| format!("nueva: JSON no válido ({e})"))?;
            &leido
        }
        None => n,
    };
    let s = |k: &str| n.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("").to_owned();
    let capitulo = s("capitulo");
    if capitulo.is_empty() {
        return Err("nueva: indique el capítulo".into());
    }
    let cantidad = dec_campo(n, "cantidad")?.ok_or("nueva: indique la cantidad")?;
    Ok(Some(ppto_core::propuesta::NuevaPartida {
        capitulo,
        capitulo_resumen: s("capitulo_resumen"),
        despues_de: Some(s("despues_de")).filter(|d| !d.is_empty()),
        unidad: s("unidad"),
        resumen: s("resumen"),
        texto: Some(s("texto")).filter(|t| !t.is_empty()),
        cantidad,
    }))
}

/// Máximo de propuestas por llamada a `proponer_lote`.
const MAX_LOTE: usize = 40;

/// Varias propuestas en una llamada: recursos comunes definidos una vez y
/// líneas abreviadas. Cada propuesta pasa por `proponer` (misma validación).
fn proponer_lote(estado: &mut Estado, a: &Value) -> Result<String, String> {
    let props = a
        .get("propuestas")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
        .ok_or("indique propuestas (lista de {partida, lineas})")?;
    if props.len() > MAX_LOTE {
        return Err(format!(
            "{} propuestas: como mucho {MAX_LOTE} por llamada; divida el lote",
            props.len()
        ));
    }
    let mut defs: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for r in a.get("recursos").and_then(Value::as_array).into_iter().flatten() {
        let c = texto(r, "codigo")?.trim().to_owned();
        defs.insert(c, r.clone());
    }
    let ma = dec_campo(a, "medios_auxiliares")?;
    let (mut bien, mut mal) = (Vec::new(), Vec::new());
    let mut netos: Vec<String> = Vec::new();
    for (i, pr) in props.iter().enumerate() {
        let partida = pr
            .get("partida")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_owned();
        let r = (|| -> Result<String, String> {
            if partida.is_empty() {
                return Err(format!("propuesta {} sin partida", i + 1));
            }
            let mut lineas = Vec::new();
            for l in pr.get("lineas").and_then(Value::as_array).ok_or("sin lineas")? {
                let mut obj = match l {
                    Value::Array(par) if par.len() == 2 => {
                        json!({ "codigo": par[0].as_str().ok_or("par sin código")?, "rendimiento": par[1] })
                    }
                    Value::Object(_) => l.clone(),
                    _ => return Err("cada línea es {codigo, rendimiento} o [\"CODIGO\", rendimiento]".into()),
                };
                let codigo = texto(&obj, "codigo")?.trim().to_owned();
                if let Some(d) = defs.get(&codigo)
                    && let (Some(o), Some(dd)) = (obj.as_object_mut(), d.as_object())
                {
                    for (k, v) in dd {
                        o.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                lineas.push(obj);
            }
            if let Some(ma) = ma
                && !lineas
                    .iter()
                    .any(|l| l["codigo"].as_str().is_some_and(|c| c.starts_with('%')))
            {
                lineas.push(json!({
                    "codigo": "%MA", "rendimiento": ma.to_string(),
                    "naturaleza": "porcentaje", "resumen": "Medios auxiliares"
                }));
            }
            let mut uno = json!({
                "partida": partida,
                "lineas": lineas,
                "confianza": pr.get("confianza").or_else(|| a.get("confianza")).cloned().unwrap_or(Value::Null),
                "justificacion": pr.get("justificacion").or_else(|| a.get("justificacion")).cloned().unwrap_or(Value::Null),
            });
            if let Some(d) = a.get("descuentos") {
                uno["descuentos"] = d.clone();
            }
            if let Some(n) = pr.get("nueva") {
                uno["nueva"] = n.clone();
            }
            proponer(estado, &uno)
        })();
        match r {
            Ok(t) => {
                bien.push(t.lines().next().unwrap_or("").to_owned());
                // Materiales a neto: se juntan en un solo aviso al final
                for l in t.lines() {
                    if let Some(resto) = l.strip_prefix(AVISO_NETO) {
                        for c in resto
                            .split(". ")
                            .next()
                            .unwrap_or("")
                            .split(", ")
                            .filter(|c| !c.is_empty())
                        {
                            if !netos.iter().any(|x: &String| x == c) {
                                netos.push(c.to_owned());
                            }
                        }
                    }
                }
            }
            Err(e) => mal.push(format!("{partida}: {e}")),
        }
    }
    let mut t = format!("{} propuestas guardadas, {} con error.", bien.len(), mal.len());
    for b in &bien {
        let _ = write!(t, "\n- {b}");
    }
    if !netos.is_empty() {
        let _ = write!(
            t,
            "\n{AVISO_NETO}{}. El usuario quiere ver siempre PVP − descuento: vuelva a proponer los de \
             catálogo con pvp, fabricante (y grupo) y el descuento.",
            netos.join(", ")
        );
    }
    if !mal.is_empty() {
        t.push_str("\nCon error (corríjalas y vuelva a enviarlas):");
        for m in &mal {
            let _ = write!(t, "\n- {m}");
        }
    }
    if bien.is_empty() {
        return Err(t);
    }
    Ok(t)
}

/// `descuentos`: [{fabricante, porcentaje}] o {"FABRICANTE": porcentaje}.
fn leer_descuentos(a: &Value) -> Result<std::collections::BTreeMap<String, Decimal>, String> {
    let mut m = std::collections::BTreeMap::new();
    match a.get("descuentos") {
        None | Some(Value::Null) => {}
        Some(Value::Array(v)) => {
            for d in v {
                let f = texto(d, "fabricante")?;
                let pct = cadena_campo(d, "porcentaje")?
                    .map(|c| ppto_core::descuentos::equivalente(&c))
                    .ok_or_else(|| format!("descuento de {f} sin porcentaje"))?;
                let (fa, gr) = ppto_core::descuentos::partes(f);
                m.insert(ppto_core::descuentos::clave(&fa, gr.as_deref()), pct);
            }
        }
        Some(Value::Object(o)) => {
            for (f, v) in o {
                let pct = cadena_campo(&json!({ "p": v }), "p")?
                    .map(|c| ppto_core::descuentos::equivalente(&c))
                    .ok_or_else(|| format!("descuento de {f} no válido"))?;
                let (fa, gr) = ppto_core::descuentos::partes(f);
                m.insert(ppto_core::descuentos::clave(&fa, gr.as_deref()), pct);
            }
        }
        Some(_) => return Err("descuentos: lista de {fabricante, porcentaje}".into()),
    }
    Ok(m)
}

fn ver_propuestas(estado: &Estado) -> Result<String, String> {
    use ppto_core::propuesta::EstadoPropuesta as E;
    let obra = estado.obra_activa.as_deref().ok_or("use antes abrir_obra_activa")?;
    let ruta = estado
        .ruta_bandeja
        .as_ref()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let b = ppto_propuestas::Bandeja::cargar(ruta);
    let v = b.de_obra(obra);
    if v.is_empty() {
        return Ok("No hay propuestas para esta obra.".into());
    }
    let mut t = String::from("| # | Partida | Autor | Estado | Confianza |\n|---:|---|---|---|---|\n");
    for p in &v {
        let e = match p.estado {
            E::Pendiente => "pendiente",
            E::Aceptada => "aceptada",
            E::Rechazada => "rechazada",
        };
        let _ = writeln!(
            t,
            "| {} | {} | {} | {e} | {} |",
            p.id,
            p.partida,
            p.autor,
            p.confianza.nombre()
        );
    }
    let mut com = String::new();
    for p in &v {
        for c in b.comentarios(p.id) {
            let postura = match c.de_acuerdo {
                Some(true) => " (de acuerdo)",
                Some(false) => " (en contra)",
                None => "",
            };
            let _ = writeln!(com, "- #{} {}{postura}: {}", p.id, c.autor, c.texto);
        }
    }
    if !com.is_empty() {
        t.push_str("\nComentarios:\n");
        t.push_str(&com);
    }
    let d = b.desacuerdos(obra);
    if !d.is_empty() {
        t.push_str(
            "\nDesacuerdos (propuestas pendientes de varios agentes para la misma partida; el usuario elige):\n",
        );
        let p = estado.p().ok();
        for (partida, ps) in &d {
            let v: Vec<String> = ps
                .iter()
                .map(|x| {
                    let precio = p
                        .and_then(|p| p.simular_propuesta(x).ok())
                        .map(|s| format!(" {} €", ppto_core::formato::eur(s.precio_despues)))
                        .unwrap_or_default();
                    format!("#{} {}{precio}", x.id, x.autor)
                })
                .collect();
            let _ = writeln!(t, "- {partida}: {}", v.join(" · "));
        }
    }
    Ok(t)
}

fn comentar_propuesta(estado: &Estado, a: &Value) -> Result<String, String> {
    let obra = estado.obra_activa.as_deref().ok_or("use antes abrir_obra_activa")?;
    let ruta = estado
        .ruta_bandeja
        .as_ref()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let id = a
        .get("id")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|t| t.trim().trim_start_matches('#').parse().ok()))
        })
        .ok_or("indique id (el número de la propuesta)")?;
    let texto = texto(a, "texto")?.trim().to_owned();
    if texto.is_empty() {
        return Err("indique el motivo en texto".into());
    }
    let de_acuerdo = a.get("de_acuerdo").and_then(Value::as_bool);
    let autor = estado.autor.clone();
    let hecho = ppto_propuestas::Bandeja::modificar(ruta, |b| {
        if b.de_obra(obra).iter().any(|p| p.id == id) {
            b.comentar(id, &autor, &texto, de_acuerdo)
        } else {
            false
        }
    })
    .map_err(|e| e.to_string())?;
    if !hecho {
        return Err(format!("no hay propuesta #{id} en esta obra"));
    }
    Ok(format!(
        "Comentario de {autor} en la propuesta #{id}{}.",
        match de_acuerdo {
            Some(true) => " (de acuerdo)",
            Some(false) => " (en contra)",
            None => "",
        }
    ))
}

#[cfg(test)]
mod pruebas_decimal {
    use super::decimal;
    use rust_decimal_macros::dec;
    use serde_json::json;

    #[test]
    fn un_punto_en_texto_es_decimal() {
        assert_eq!(decimal(&json!("7.518")), Some(dec!(7.518)));
        assert_eq!(decimal(&json!(7.518)), Some(dec!(7.518)));
        assert_eq!(decimal(&json!("7,518")), Some(dec!(7.518)));
        assert_eq!(decimal(&json!("1.234,56")), Some(dec!(1234.56)));
        assert_eq!(decimal(&json!("1.234.567")), Some(dec!(1234567)));
        assert_eq!(decimal(&json!("45")), Some(dec!(45)));
    }
}
