// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-mcp
//!
//! Servidor MCP (Model Context Protocol, JSON-RPC 2.0 por stdio) para que un
//! asistente como Claude consulte y simule presupuestos (P-020).
//!
//! **Solo lectura y simulación**: ninguna herramienta modifica el presupuesto
//! abierto ni el fichero de origen. Las simulaciones trabajan sobre copias.
//! Las escrituras con aprobación llegarán en P-021. Las exportaciones crean
//! ficheros nuevos y nunca sobrescriben el de origen.
//!
//! Todas las cifras salen de `ppto-core`, igual que en la interfaz y los informes.

use ppto_core::concepto::Naturaleza;
use ppto_core::formato::{eur, num};
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, simular};
use ppto_core::{Decimal, Presupuesto, venta};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::str::FromStr;

/// Versión del protocolo MCP que se anuncia si el cliente no pide otra.
pub const PROTOCOLO: &str = "2025-06-18";

/// Presupuesto abierto y porcentajes del resumen.
pub struct Estado {
    pub presupuesto: Option<Presupuesto>,
    pub origen: String,
    pub gg: Decimal,
    pub bi: Decimal,
    pub iva: Decimal,
}

impl Default for Estado {
    fn default() -> Self {
        Self {
            presupuesto: None,
            origen: String::new(),
            gg: venta::GG_HABITUAL,
            bi: venta::BI_HABITUAL,
            iva: venta::IVA_GENERAL,
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
        let p = self.presupuesto.as_ref().ok_or("sin presupuesto")?;
        let _ = writeln!(
            informe,
            "{} — PEM {} €",
            p.nombre,
            eur(p.pem().map_err(|e| e.to_string())?)
        );
        Ok(informe)
    }

    fn p(&self) -> Result<&Presupuesto, String> {
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
                    modifican el presupuesto. Importes en euros con formato español."
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
            "name": "exportar_excel",
            "description": "Genera el informe Excel (resumen, presupuesto, descompuestos, mediciones, recursos y horas por oficio) en un fichero nuevo.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string", "description": "Ruta del .xlsx a crear" } }, "required": ["ruta"] }
        },
        {
            "name": "exportar_bc3",
            "description": "Exporta el presupuesto a BC3 (FIEBDC-3/2002, ANSI, como Presto 8.8) en un fichero nuevo.",
            "inputSchema": { "type": "object", "properties": { "ruta": { "type": "string", "description": "Ruta del .bc3 a crear" } }, "required": ["ruta"] }
        }
    ])
}

fn llamar(estado: &mut Estado, nombre: &str, a: &Value) -> Result<String, String> {
    match nombre {
        "abrir_presupuesto" => estado.abrir(texto(a, "ruta")?),
        "abrir_ejemplo" => {
            estado.presupuesto = Some(ppto_core::ejemplos::suelo_radiante());
            estado.origen = "ejemplo".into();
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

fn texto<'a>(a: &'a Value, campo: &str) -> Result<&'a str, String> {
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
        Value::String(s) => ppto_core::formato::leer(s),
        _ => None,
    }
}

fn dec_campo(a: &Value, campo: &str) -> Result<Option<Decimal>, String> {
    match a.get(campo) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => decimal(v)
            .map(Some)
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
