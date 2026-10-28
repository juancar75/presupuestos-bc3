// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-biblioteca
//!
//! Biblioteca de precios propia del usuario, que crece con cada obra:
//!
//! - **Partidas con descompuesto** (con versiones: fecha, obra de origen y
//!   si el precio es real o estimado).
//! - Por **fabricante**, tres cosas separadas:
//!   - **descuentos** del instalador (general y por grupo de producto), que
//!     no caducan;
//!   - **tarifas por año** (PVP por referencia); se usa la más reciente y se
//!     avisa cuando se queda antigua;
//!   - **ofertas en netos** para una obra concreta (con validez).
//!
//! Las obras ya presupuestadas no cambian al cargar una tarifa nueva: la
//! biblioteca solo se usa al valorar partidas nuevas.
//!
//! Se guarda en `biblioteca.json` en la carpeta de configuración, o en la
//! carpeta indicada en `biblioteca-ruta.txt` (p. ej. una carpeta compartida
//! de OneDrive o del NAS). **Los precios reales no van al repositorio.**

use ppto_core::concepto::Naturaleza;
use ppto_core::descuentos::{clave, normalizar, partes};
use ppto_core::{Decimal, Presupuesto};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FICHERO: &str = "biblioteca.json";

/// Ruta de la biblioteca: la carpeta de `biblioteca-ruta.txt` (compartida)
/// o la carpeta de configuración.
pub fn ruta() -> Option<PathBuf> {
    let cfg = ppto_propuestas::carpeta_config()?;
    let carpeta = std::fs::read_to_string(cfg.join("biblioteca-ruta.txt"))
        .ok()
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or(cfg);
    Some(carpeta.join(FICHERO))
}

/// Fecha de hoy `AAAA-MM-DD` (UTC).
pub fn hoy() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    fecha_de(i64::try_from(s / 86_400).unwrap_or(0))
}

/// `AAAA-MM-DD` a partir de días desde 1970-01-01 (algoritmo de H. Hinnant).
fn fecha_de(dias: i64) -> String {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{a:04}-{m:02}-{d:02}")
}

/// Año de una fecha `AAAA-…`.
pub fn anio(fecha: &str) -> i32 {
    fecha.get(..4).and_then(|a| a.parse().ok()).unwrap_or(0)
}

// ---------------------------------------------------------------- modelo

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origen {
    /// Obra cerrada, oferta o pedido real.
    Real,
    /// Revisado por el usuario (p. ej. propuesta de la IA aceptada).
    #[default]
    Revisado,
    /// Estimación sin contrastar.
    Estimado,
}

impl Origen {
    pub fn texto(self) -> &'static str {
        match self {
            Origen::Real => "real",
            Origen::Revisado => "revisado",
            Origen::Estimado => "estimado",
        }
    }
}

/// Línea del descompuesto guardado (con los datos del recurso, para poder
/// crearlo en otra obra).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaBib {
    pub codigo: String,
    pub naturaleza: Naturaleza,
    pub unidad: String,
    pub resumen: String,
    /// Cantidad por unidad de partida (rendimiento × factor); en porcentajes, los puntos.
    pub rendimiento: Decimal,
    /// Precio neto unitario al guardar (0 en porcentajes).
    pub precio: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fabricante: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grupo: Option<String>,
    /// PVP de tarifa al guardar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pvp: Option<Decimal>,
    /// Referencia en la tarifa del fabricante (por defecto, el código).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referencia: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionPartida {
    pub fecha: String,
    pub obra: String,
    pub unidad: String,
    pub resumen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texto: Option<String>,
    pub origen: Origen,
    pub lineas: Vec<LineaBib>,
    /// Precio unitario (coste directo) al guardar.
    pub precio: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaPvp {
    pub descripcion: String,
    #[serde(default)]
    pub unidad: String,
    pub pvp: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grupo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TarifaFabricante {
    /// «2026», «2026-1»…
    pub etiqueta: String,
    pub anio: i32,
    /// Fecha de carga.
    pub cargada: String,
    /// Referencia → línea.
    pub lineas: BTreeMap<String, LineaPvp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaOferta {
    pub referencia: String,
    pub descripcion: String,
    #[serde(default)]
    pub unidad: String,
    pub neto: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cantidad: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Oferta {
    pub numero: String,
    pub fecha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validez: Option<String>,
    #[serde(default)]
    pub obra: String,
    pub lineas: Vec<LineaOferta>,
    /// Ruta del documento de la oferta, la que se elija en cada caso (carpeta
    /// de la obra, unidad compartida…). No se copia.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documento: Option<String>,
}

impl Oferta {
    pub fn vencida(&self, hoy: &str) -> bool {
        self.validez.as_deref().is_some_and(|v| !v.is_empty() && v < hoy)
    }

    /// Procedencia de precio para la línea `referencia` de esta oferta
    /// (`None`: la oferta entera, sin artículo concreto).
    pub fn origen(&self, fabricante: &str, referencia: Option<&str>) -> ppto_core::OrigenPrecio {
        let l = referencia.and_then(|r| self.lineas.iter().find(|l| l.referencia == r));
        ppto_core::OrigenPrecio {
            fabricante: fabricante.to_owned(),
            oferta: self.numero.clone(),
            fecha: self.fecha.clone(),
            validez: self.validez.clone(),
            referencia: l.map(|l| l.referencia.clone()),
            neto: l.map(|l| l.neto),
            documento: self.documento.clone(),
        }
    }
}

/// Datos del fabricante o proveedor (para pedidos y para justificar precios).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosFabricante {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub razon_social: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cif: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contacto: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub telefono: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// Plazos, portes, forma de pago…
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condiciones: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Fabricante {
    pub nombre: String,
    /// Grupo de producto ("" = general) → descuento en puntos.
    #[serde(default)]
    pub descuentos: BTreeMap<String, Decimal>,
    /// Ordenadas de más antigua a más reciente.
    #[serde(default)]
    pub tarifas: Vec<TarifaFabricante>,
    #[serde(default)]
    pub ofertas: Vec<Oferta>,
    /// Subida estimada (puntos) sobre la última tarifa mientras no llegue la nueva.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subida_estimada: Option<Decimal>,
    #[serde(default)]
    pub datos: DatosFabricante,
}

impl Fabricante {
    pub fn vigente(&self) -> Option<&TarifaFabricante> {
        self.tarifas.last()
    }

    /// Descuento del grupo si lo hay; si no, el general.
    pub fn descuento(&self, grupo: Option<&str>) -> Decimal {
        grupo
            .map(normalizar)
            .and_then(|g| self.descuentos.get(&g).copied())
            .or_else(|| self.descuentos.get("").copied())
            .unwrap_or_default()
    }
}

/// PVP vigente de una referencia.
#[derive(Debug, Clone, PartialEq)]
pub struct Pvp {
    pub pvp: Decimal,
    pub tarifa: String,
    /// Con la subida estimada aplicada (la tarifa es de un año anterior).
    pub estimado: bool,
    pub grupo: Option<String>,
    pub descripcion: String,
}

/// Resultado de cargar una tarifa nueva frente a la anterior.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparacion {
    pub lineas: usize,
    /// Variación media en % de las referencias comunes (None si no había tarifa anterior).
    pub media: Option<Decimal>,
    /// (referencia, pvp anterior, pvp nuevo, %) de las que más suben.
    pub mayores: Vec<(String, Decimal, Decimal, Decimal)>,
    pub nuevas: Vec<String>,
    pub desaparecidas: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InformeGuardado {
    pub nuevas: Vec<String>,
    pub actualizadas: Vec<String>,
    pub iguales: usize,
    pub descuentos: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Biblioteca {
    #[serde(default)]
    pub fabricantes: BTreeMap<String, Fabricante>,
    /// Código → versiones (de más antigua a más reciente).
    #[serde(default)]
    pub partidas: BTreeMap<String, Vec<VersionPartida>>,
}

/// Texto en minúsculas sin tildes, para buscar.
pub fn plano(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'Á' | 'À' | 'Ä' => 'a',
            'é' | 'è' | 'ë' | 'É' | 'È' | 'Ë' => 'e',
            'í' | 'ì' | 'ï' | 'Í' | 'Ì' | 'Ï' => 'i',
            'ó' | 'ò' | 'ö' | 'Ó' | 'Ò' | 'Ö' => 'o',
            'ú' | 'ù' | 'ü' | 'Ú' | 'Ù' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn coincidencias(texto: &str, palabras: &[String]) -> usize {
    let t = plano(texto);
    palabras.iter().filter(|p| t.contains(p.as_str())).count()
}

impl Biblioteca {
    // ------------------------------------------------------------ disco

    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Lee, modifica y guarda con el fichero bloqueado. Un fichero que existe
    /// pero no se puede leer no se sobrescribe.
    pub fn modificar<R>(ruta: &Path, f: impl FnOnce(&mut Self) -> R) -> std::io::Result<R> {
        ppto_propuestas::con_bloqueo(ruta, || {
            let mut b: Self = match std::fs::read_to_string(ruta) {
                Ok(t) if t.trim().is_empty() => Self::default(),
                Ok(t) => serde_json::from_str(&t).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{} dañada: {e}", ruta.display()),
                    )
                })?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
                Err(e) => return Err(e),
            };
            let r = f(&mut b);
            let t = serde_json::to_string_pretty(&b).map_err(std::io::Error::other)?;
            let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
            std::fs::write(&tmp, t)?;
            std::fs::rename(&tmp, ruta)?;
            Ok(r)
        })
    }

    // ------------------------------------------------------------ fabricantes

    pub fn fabricante(&self, nombre: &str) -> Option<&Fabricante> {
        self.fabricantes.get(&normalizar(nombre))
    }

    fn fabricante_mut(&mut self, nombre: &str) -> &mut Fabricante {
        let k = normalizar(nombre);
        self.fabricantes.entry(k.clone()).or_insert_with(|| Fabricante {
            nombre: k,
            ..Fabricante::default()
        })
    }

    /// Datos del fabricante o proveedor (lo crea si no existe).
    pub fn fijar_datos(&mut self, fabricante: &str, datos: DatosFabricante) {
        self.fabricante_mut(fabricante).datos = datos;
    }

    /// Descuento de un fabricante (o «FAB/GRUPO»). `None` lo quita.
    pub fn fijar_descuento(&mut self, fabricante: &str, pct: Option<Decimal>) {
        let (f, g) = partes(fabricante);
        let fab = self.fabricante_mut(&f);
        let g = g.unwrap_or_default();
        match pct {
            Some(p) => {
                fab.descuentos.insert(g, p);
            }
            None => {
                fab.descuentos.remove(&g);
            }
        }
    }

    pub fn fijar_subida(&mut self, fabricante: &str, pct: Option<Decimal>) {
        self.fabricante_mut(fabricante).subida_estimada = pct;
    }

    /// Carga una tarifa (PVP por referencia) y la compara con la anterior.
    /// Si ya existía una tarifa con la misma etiqueta, se sustituye.
    pub fn cargar_tarifa(
        &mut self,
        fabricante: &str,
        etiqueta: &str,
        anio: i32,
        lineas: BTreeMap<String, LineaPvp>,
    ) -> Comparacion {
        let fab = self.fabricante_mut(fabricante);
        fab.tarifas.retain(|t| t.etiqueta != etiqueta);
        let anterior = fab.tarifas.last().cloned();
        let mut cmp = Comparacion {
            lineas: lineas.len(),
            ..Comparacion::default()
        };
        if let Some(ant) = &anterior {
            let mut suma = Decimal::ZERO;
            let mut n = 0u32;
            let mut variaciones = Vec::new();
            for (r, l) in &lineas {
                match ant.lineas.get(r) {
                    Some(a) if !a.pvp.is_zero() => {
                        let pct = ((l.pvp - a.pvp) / a.pvp * Decimal::ONE_HUNDRED).round_dp(2);
                        suma += pct;
                        n += 1;
                        variaciones.push((r.clone(), a.pvp, l.pvp, pct));
                    }
                    Some(_) => {}
                    None => cmp.nuevas.push(r.clone()),
                }
            }
            cmp.desaparecidas = ant
                .lineas
                .keys()
                .filter(|r| !lineas.contains_key(*r))
                .cloned()
                .collect();
            if n > 0 {
                cmp.media = Some((suma / Decimal::from(n)).round_dp(2));
            }
            variaciones.sort_by_key(|v| std::cmp::Reverse(v.3));
            variaciones.truncate(10);
            cmp.mayores = variaciones;
        }
        fab.tarifas.push(TarifaFabricante {
            etiqueta: etiqueta.to_owned(),
            anio,
            cargada: hoy(),
            lineas,
        });
        fab.tarifas
            .sort_by(|a, b| (a.anio, &a.etiqueta).cmp(&(b.anio, &b.etiqueta)));
        // Una tarifa nueva hace innecesaria la subida estimada.
        if fab.tarifas.last().is_some_and(|t| t.etiqueta == etiqueta) {
            fab.subida_estimada = None;
        }
        cmp
    }

    /// PVP vigente de una referencia de un fabricante. Si la tarifa más
    /// reciente es de un año anterior a `anio_actual` y hay subida estimada,
    /// se aplica y se marca como estimado.
    pub fn pvp(&self, fabricante: &str, referencia: &str, anio_actual: i32) -> Option<Pvp> {
        let fab = self.fabricante(fabricante)?;
        let t = fab.tarifas.iter().rev().find(|t| t.lineas.contains_key(referencia))?;
        let l = &t.lineas[referencia];
        let es_ultima = fab.vigente().is_some_and(|v| v.etiqueta == t.etiqueta);
        let (pvp, estimado) = match fab.subida_estimada {
            Some(s) if es_ultima && t.anio < anio_actual => (
                (l.pvp * (Decimal::ONE_HUNDRED + s) / Decimal::ONE_HUNDRED).round_dp(2),
                true,
            ),
            _ => (l.pvp, false),
        };
        Some(Pvp {
            pvp,
            tarifa: t.etiqueta.clone(),
            estimado,
            grupo: l.grupo.clone(),
            descripcion: l.descripcion.clone(),
        })
    }

    /// Fabricantes cuya tarifa más reciente es de un año anterior: (nombre, etiqueta, año).
    pub fn tarifas_antiguas(&self, anio_actual: i32) -> Vec<(String, String, i32)> {
        self.fabricantes
            .values()
            .filter_map(|f| f.vigente().map(|t| (f, t)))
            .filter(|(_, t)| t.anio < anio_actual)
            .map(|(f, t)| (f.nombre.clone(), t.etiqueta.clone(), t.anio))
            .collect()
    }

    pub fn anadir_oferta(&mut self, fabricante: &str, oferta: Oferta) {
        let fab = self.fabricante_mut(fabricante);
        fab.ofertas
            .retain(|o| o.numero != oferta.numero || oferta.numero.is_empty());
        fab.ofertas.push(oferta);
    }

    /// Vincula (o desvincula) el documento de una oferta: la ruta que se elija.
    pub fn fijar_documento_oferta(&mut self, fabricante: &str, numero: &str, ruta: Option<String>) -> bool {
        let ruta = ruta
            .map(|r| r.trim().trim_matches('"').to_owned())
            .filter(|r| !r.is_empty());
        let Some(of) = self
            .fabricantes
            .get_mut(&normalizar(fabricante))
            .and_then(|f| f.ofertas.iter_mut().find(|o| o.numero == numero))
        else {
            return false;
        };
        of.documento = ruta;
        true
    }

    /// Descuento real de una oferta frente a la tarifa vigente: por grupo
    /// ("" sin grupo), media de 1 − neto/PVP en puntos, con el nº de líneas.
    pub fn descuento_de_oferta(&self, fabricante: &str, numero: &str) -> BTreeMap<String, (Decimal, usize)> {
        let mut acc: BTreeMap<String, (Decimal, usize)> = BTreeMap::new();
        let Some(fab) = self.fabricante(fabricante) else {
            return acc;
        };
        let Some(of) = fab.ofertas.iter().find(|o| o.numero == numero) else {
            return acc;
        };
        for l in &of.lineas {
            let Some(t) = fab.tarifas.iter().rev().find(|t| t.lineas.contains_key(&l.referencia)) else {
                continue;
            };
            let lp = &t.lineas[&l.referencia];
            if lp.pvp.is_zero() {
                continue;
            }
            let dto = (Decimal::ONE - l.neto / lp.pvp) * Decimal::ONE_HUNDRED;
            let e = acc.entry(lp.grupo.clone().unwrap_or_default()).or_default();
            e.0 += dto;
            e.1 += 1;
        }
        acc.into_iter()
            .map(|(g, (s, n))| (g, ((s / Decimal::from(n as u64)).round_dp(2), n)))
            .collect()
    }

    // ------------------------------------------------------------ partidas

    /// Guarda en la biblioteca las partidas descompuestas con precio de una
    /// obra (o solo las indicadas). Una partida igual a su última versión no
    /// se duplica. También aprende los descuentos de la obra.
    pub fn guardar_obra(
        &mut self,
        p: &Presupuesto,
        obra: &str,
        origen: Origen,
        solo: Option<&[String]>,
    ) -> InformeGuardado {
        let mut inf = InformeGuardado::default();
        let fecha = hoy();
        for c in p.conceptos.values() {
            if c.naturaleza != Naturaleza::Partida || c.descomposicion.is_empty() {
                continue;
            }
            if solo.is_some_and(|s| !s.iter().any(|x| x == &c.codigo)) {
                continue;
            }
            let Ok(precio) = p.precio(&c.codigo) else { continue };
            if precio.is_zero() {
                continue;
            }
            let mut lineas = Vec::new();
            for l in &c.descomposicion {
                let Some(h) = p.conceptos.get(&l.hijo) else { continue };
                let unit = if h.naturaleza == Naturaleza::Porcentaje {
                    Decimal::ZERO
                } else {
                    p.precio(&h.codigo).unwrap_or_default()
                };
                lineas.push(LineaBib {
                    codigo: h.codigo.clone(),
                    naturaleza: if h.naturaleza == Naturaleza::Partida {
                        Naturaleza::Otros
                    } else {
                        h.naturaleza
                    },
                    unidad: h.unidad.clone(),
                    resumen: h.resumen.clone(),
                    rendimiento: l.rendimiento * l.factor,
                    precio: unit,
                    fabricante: h.tarifa.as_ref().map(|t| t.fabricante.clone()),
                    grupo: h.tarifa.as_ref().and_then(|t| t.grupo.clone()),
                    pvp: h.tarifa.as_ref().map(|t| t.pvp),
                    referencia: None,
                });
            }
            let v = VersionPartida {
                fecha: fecha.clone(),
                obra: obra.to_owned(),
                unidad: c.unidad.clone(),
                resumen: c.resumen.clone(),
                texto: c.texto.clone(),
                origen,
                lineas,
                precio,
            };
            let versiones = self.partidas.entry(c.codigo.clone()).or_default();
            match versiones.last() {
                Some(u) if u.lineas == v.lineas && u.precio == v.precio => inf.iguales += 1,
                Some(_) => {
                    versiones.push(v);
                    inf.actualizadas.push(c.codigo.clone());
                }
                None => {
                    versiones.push(v);
                    inf.nuevas.push(c.codigo.clone());
                }
            }
        }
        for (k, pct) in &p.descuentos {
            let (f, g) = partes(k);
            let fab = self.fabricante_mut(&f);
            let g = g.unwrap_or_default();
            if fab.descuentos.get(&g) != Some(pct) {
                fab.descuentos.insert(g.clone(), *pct);
                inf.descuentos
                    .push(clave(&f, Some(&g).filter(|g| !g.is_empty()).map(String::as_str)));
            }
        }
        inf
    }

    pub fn ultima(&self, codigo: &str) -> Option<&VersionPartida> {
        self.partidas.get(codigo).and_then(|v| v.last())
    }

    /// Partidas cuyo código, resumen o texto contienen las palabras (sin
    /// tildes ni mayúsculas), ordenadas por coincidencias.
    pub fn buscar(&self, texto: &str, limite: usize) -> Vec<(&str, &VersionPartida)> {
        let palabras: Vec<String> = plano(texto).split_whitespace().map(str::to_owned).collect();
        if palabras.is_empty() {
            return Vec::new();
        }
        let mut v: Vec<(usize, &str, &VersionPartida)> = self
            .partidas
            .iter()
            .filter_map(|(c, vs)| vs.last().map(|u| (c.as_str(), u)))
            .map(|(c, u)| {
                let t = format!("{c} {} {}", u.resumen, u.texto.as_deref().unwrap_or(""));
                (coincidencias(&t, &palabras), c, u)
            })
            .filter(|(n, _, _)| *n == palabras.len())
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        v.into_iter().take(limite).map(|(_, c, u)| (c, u)).collect()
    }

    /// Líneas de tarifas y ofertas que contienen las palabras:
    /// (fabricante, «tarifa 2026»/«oferta 123», referencia, descripción, precio, es_neto).
    pub fn buscar_precios(&self, texto: &str, limite: usize) -> Vec<(String, String, String, String, Decimal, bool)> {
        let palabras: Vec<String> = plano(texto).split_whitespace().map(str::to_owned).collect();
        let mut v = Vec::new();
        if palabras.is_empty() {
            return v;
        }
        for f in self.fabricantes.values() {
            if let Some(t) = f.vigente() {
                for (r, l) in &t.lineas {
                    if coincidencias(&format!("{} {r} {}", f.nombre, l.descripcion), &palabras) == palabras.len() {
                        v.push((
                            f.nombre.clone(),
                            format!("tarifa {}", t.etiqueta),
                            r.clone(),
                            l.descripcion.clone(),
                            l.pvp,
                            false,
                        ));
                    }
                }
            }
            for o in &f.ofertas {
                for l in &o.lineas {
                    if coincidencias(&format!("{} {} {}", f.nombre, l.referencia, l.descripcion), &palabras)
                        == palabras.len()
                    {
                        v.push((
                            f.nombre.clone(),
                            format!("oferta {} ({})", o.numero, o.fecha),
                            l.referencia.clone(),
                            l.descripcion.clone(),
                            l.neto,
                            true,
                        ));
                    }
                }
            }
        }
        v.truncate(limite);
        v
    }

    /// Precio de hoy de una línea guardada: si es de un fabricante con
    /// tarifa en la biblioteca, PVP vigente × (1 − descuento). Devuelve
    /// (neto, pvp, nota).
    pub fn precio_hoy(&self, l: &LineaBib, anio_actual: i32) -> (Decimal, Option<Decimal>, Option<String>) {
        let Some(fab) = &l.fabricante else {
            return (l.precio, None, None);
        };
        let referencia = l.referencia.as_deref().unwrap_or(&l.codigo);
        let dto = self
            .fabricante(fab)
            .map(|f| f.descuento(l.grupo.as_deref()))
            .unwrap_or_default();
        match self.pvp(fab, referencia, anio_actual) {
            Some(p) => {
                let neto = (p.pvp * (Decimal::ONE_HUNDRED - dto) / Decimal::ONE_HUNDRED).round_dp(2);
                let nota = format!(
                    "tarifa {}{} − {} %",
                    p.tarifa,
                    if p.estimado { " + subida estimada" } else { "" },
                    dto.normalize()
                );
                (neto, Some(p.pvp), Some(nota))
            }
            None => (
                l.precio,
                l.pvp,
                Some("sin tarifa en la biblioteca: precio guardado".into()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn lp(desc: &str, pvp: Decimal, grupo: Option<&str>) -> LineaPvp {
        LineaPvp {
            descripcion: desc.into(),
            unidad: "ud".into(),
            pvp,
            grupo: grupo.map(str::to_owned),
        }
    }

    #[test]
    fn fechas() {
        assert_eq!(fecha_de(0), "1970-01-01");
        assert_eq!(fecha_de(20_744), "2026-10-18");
        assert_eq!(anio("2026-10-18"), 2026);
    }

    #[test]
    fn tarifas_por_anio_descuento_que_no_caduca_y_subida_estimada() {
        let mut b = Biblioteca::default();
        b.fijar_descuento("Koolair", Some(dec!(45)));
        let t26 = BTreeMap::from([
            ("20-H-400x300".to_owned(), lp("Rejilla 20-H 400x300", dec!(100), None)),
            ("VIEJA".to_owned(), lp("Descatalogada", dec!(50), None)),
        ]);
        let c = b.cargar_tarifa("KOOLAIR", "2026", 2026, t26);
        assert_eq!(c.media, None);
        let l = LineaBib {
            codigo: "20-H-400x300".into(),
            naturaleza: Naturaleza::Material,
            unidad: "ud".into(),
            resumen: "Rejilla".into(),
            rendimiento: dec!(1),
            precio: dec!(55),
            fabricante: Some("KOOLAIR".into()),
            grupo: None,
            pvp: Some(dec!(100)),
            referencia: None,
        };
        assert_eq!(b.precio_hoy(&l, 2026).0, dec!(55));
        // En 2027 sin tarifa nueva: aviso y subida estimada marcada
        assert_eq!(b.tarifas_antiguas(2027), vec![("KOOLAIR".into(), "2026".into(), 2026)]);
        b.fijar_subida("koolair", Some(dec!(4)));
        let p = b.pvp("KOOLAIR", "20-H-400x300", 2027).unwrap();
        assert!(p.estimado);
        assert_eq!(p.pvp, dec!(104));
        // Llega la tarifa 2027: mismo descuento, PVP nuevo, comparación
        let t27 = BTreeMap::from([
            ("20-H-400x300".to_owned(), lp("Rejilla 20-H 400x300", dec!(106), None)),
            ("NUEVA".to_owned(), lp("Nueva", dec!(10), None)),
        ]);
        let c = b.cargar_tarifa("koolair", "2027", 2027, t27);
        assert_eq!(c.media, Some(dec!(6)));
        assert_eq!(c.nuevas, ["NUEVA"]);
        assert_eq!(c.desaparecidas, ["VIEJA"]);
        assert!(b.tarifas_antiguas(2027).is_empty());
        let (neto, pvp, nota) = b.precio_hoy(&l, 2027);
        assert_eq!((neto, pvp), (dec!(58.30), Some(dec!(106))));
        assert!(nota.unwrap().contains("2027"));
        // La referencia desaparecida sigue disponible con su tarifa antigua
        assert_eq!(b.pvp("KOOLAIR", "VIEJA", 2027).unwrap().tarifa, "2026");
    }

    #[test]
    fn descuento_por_grupo_y_descuento_real_de_una_oferta() {
        let mut b = Biblioteca::default();
        b.fijar_descuento("S&P", Some(dec!(40)));
        b.fijar_descuento("S&P/E08", Some(dec!(50)));
        assert_eq!(b.fabricante("s&p").unwrap().descuento(Some("e08")), dec!(50));
        assert_eq!(b.fabricante("s&p").unwrap().descuento(Some("E22")), dec!(40));

        b.cargar_tarifa(
            "LG",
            "2026",
            2026,
            BTreeMap::from([
                (
                    "ARUM100".to_owned(),
                    lp("Exterior VRF 28 kW", dec!(10000), Some("VRF EXT")),
                ),
                (
                    "ARNU07".to_owned(),
                    lp("Interior cassette", dec!(1000), Some("VRF INT")),
                ),
            ]),
        );
        b.anadir_oferta(
            "LG",
            Oferta {
                numero: "OF-77".into(),
                fecha: "2026-10-10".into(),
                validez: Some("2026-11-10".into()),
                obra: "Berrocales".into(),
                documento: Some(r"\\servidor\ofertas\LG OF-77.pdf".into()),
                lineas: vec![
                    LineaOferta {
                        referencia: "ARUM100".into(),
                        descripcion: "Exterior".into(),
                        unidad: "ud".into(),
                        neto: dec!(4800),
                        cantidad: Some(dec!(2)),
                    },
                    LineaOferta {
                        referencia: "ARNU07".into(),
                        descripcion: "Cassette".into(),
                        unidad: "ud".into(),
                        neto: dec!(520),
                        cantidad: Some(dec!(10)),
                    },
                ],
            },
        );
        let o = b.fabricante("LG").unwrap().ofertas[0].origen("LG", Some("ARNU07"));
        assert_eq!(o.neto, Some(dec!(520)));
        assert_eq!(o.documento.as_deref(), Some(r"\\servidor\ofertas\LG OF-77.pdf"));
        assert!(b.fijar_documento_oferta("lg", "OF-77", Some("\"D:\\Obra X\\of.pdf\"".into())));
        assert_eq!(
            b.fabricante("LG").unwrap().ofertas[0].documento.as_deref(),
            Some(r"D:\Obra X\of.pdf")
        );
        let d = b.descuento_de_oferta("lg", "OF-77");
        assert_eq!(d["VRF EXT"], (dec!(52), 1));
        assert_eq!(d["VRF INT"], (dec!(48), 1));
        assert!(b.fabricante("LG").unwrap().ofertas[0].vencida("2026-12-01"));
        let r = b.buscar_precios("cassette", 10);
        assert_eq!(r.len(), 2); // en la tarifa y en la oferta
        assert!(r.iter().any(|x| x.5 && x.4 == dec!(520)));
    }

    #[test]
    fn guardar_obra_con_versiones_y_buscar() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        p.fijar_tarifa("MT.PANEL", "Acme", dec!(20)).unwrap();
        p.fijar_descuento("ACME", dec!(50)).unwrap();
        let mut b = Biblioteca::default();
        let i = b.guardar_obra(&p, "Ejemplo", Origen::Real, None);
        assert_eq!(i.nuevas.len(), 2);
        assert!(i.descuentos.contains(&"ACME".to_owned()));
        let v = b.ultima("SR.M2").unwrap();
        assert_eq!(v.origen, Origen::Real);
        let panel = v.lineas.iter().find(|l| l.codigo == "MT.PANEL").unwrap();
        assert_eq!(panel.fabricante.as_deref(), Some("ACME"));
        assert_eq!(panel.precio, dec!(10));
        // Guardar otra vez sin cambios no duplica
        let i = b.guardar_obra(&p, "Ejemplo", Origen::Real, None);
        assert_eq!((i.nuevas.len(), i.actualizadas.len(), i.iguales), (0, 0, 2));
        // Con un cambio, nueva versión
        p.fijar_descuento("ACME", dec!(40)).unwrap();
        let i = b.guardar_obra(&p, "Ejemplo 2", Origen::Revisado, Some(&["SR.M2".to_owned()]));
        assert_eq!(i.actualizadas, ["SR.M2"]);
        assert_eq!(b.partidas["SR.M2"].len(), 2);
        // Búsqueda sin tildes
        let r = b.buscar("suelo RADIANTE tetones", 5);
        assert_eq!(r[0].0, "SR.M2");
        assert!(b.buscar("inexistente", 5).is_empty());
    }

    #[test]
    fn fichero_danado_no_se_sobrescribe() {
        let dir = std::env::temp_dir().join(format!("bib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = dir.join(FICHERO);
        Biblioteca::modificar(&r, |b| b.fijar_descuento("X", Some(dec!(10)))).unwrap();
        assert_eq!(
            Biblioteca::cargar(&r).fabricante("x").unwrap().descuento(None),
            dec!(10)
        );
        std::fs::write(&r, "{roto").unwrap();
        assert!(Biblioteca::modificar(&r, |b| b.fijar_descuento("X", None)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
