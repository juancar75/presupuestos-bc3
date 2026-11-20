// SPDX-License-Identifier: GPL-3.0-or-later
//! Modelos IFC (texto STEP, ISO 10303-21): elementos por tipo, nombre de
//! tipo y planta, con la longitud de conductos y tuberías y la superficie de
//! los espacios cuando el modelo las trae, para comprobar las mediciones.
//!
//! Lector propio y sencillo de la sección DATA: no hace falta ninguna
//! dependencia. Vale para IFC2x3 e IFC4 (los atributos que se usan están en
//! la misma posición en los dos). Los IFC comprimidos (ifcZIP) no se leen.
//!
//! Geometría del modelo, no dinero: aquí sí se usa coma flotante (los
//! importes siguen en `Decimal` en todo el programa).
#![allow(clippy::float_arithmetic)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

/// Valor de un atributo STEP.
#[derive(Debug, Clone, PartialEq)]
enum V {
    Ref(u64),
    Txt(String),
    Num(f64),
    Enum(String),
    Lista(Vec<V>),
    /// Valor con tipo, p. ej. `IFCLENGTHMEASURE(1200.)`.
    Tipado(String, Box<V>),
    Nulo,
}

impl V {
    fn texto(&self) -> Option<&str> {
        match self {
            V::Txt(s) => Some(s.as_str()).filter(|s| !s.trim().is_empty()),
            V::Tipado(_, v) => v.texto(),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            V::Num(n) => Some(*n),
            V::Tipado(_, v) => v.num(),
            _ => None,
        }
    }
    fn refs(&self) -> Vec<u64> {
        match self {
            V::Ref(r) => vec![*r],
            V::Lista(l) => l
                .iter()
                .filter_map(|v| if let V::Ref(r) = v { Some(*r) } else { None })
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Entidad {
    tipo: String,
    args: Vec<V>,
}

impl Entidad {
    fn arg(&self, i: usize) -> &V {
        self.args.get(i).unwrap_or(&V::Nulo)
    }
}

/// Un elemento del modelo, ya resuelto.
#[derive(Debug, Clone, PartialEq)]
pub struct Elemento {
    /// Clase IFC sin el prefijo («DUCTSEGMENT», «SENSOR»…).
    pub clase: String,
    /// Nombre del tipo (o del elemento, sin el id final de Revit).
    pub nombre: String,
    pub planta: String,
    /// Tamaño o diámetro, si el modelo lo trae (segmentos).
    pub tamano: String,
    /// Longitud en m, si el modelo la trae.
    pub longitud: Option<f64>,
}

/// Un espacio (IfcSpace) con su superficie en m².
#[derive(Debug, Clone, PartialEq)]
pub struct Espacio {
    pub nombre: String,
    pub planta: String,
    pub superficie: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Modelo {
    pub esquema: String,
    pub elementos: Vec<Elemento>,
    pub espacios: Vec<Espacio>,
    /// m por unidad de longitud del modelo.
    pub escala: f64,
}

/// Clases de instalaciones (se cuentan aunque no estén en una planta).
const INSTALACIONES: &[&str] = &[
    "ACTUATOR",
    "AIRTERMINAL",
    "AIRTERMINALBOX",
    "AIRTOAIRHEATRECOVERY",
    "ALARM",
    "BOILER",
    "BURNER",
    "CABLECARRIERFITTING",
    "CABLECARRIERSEGMENT",
    "CABLEFITTING",
    "CABLESEGMENT",
    "CHILLER",
    "COIL",
    "COMMUNICATIONSAPPLIANCE",
    "COMPRESSOR",
    "CONDENSER",
    "CONTROLLER",
    "COOLINGTOWER",
    "DAMPER",
    "DISTRIBUTIONCHAMBERELEMENT",
    "DISTRIBUTIONCONTROLELEMENT",
    "DISTRIBUTIONELEMENT",
    "DISTRIBUTIONFLOWELEMENT",
    "DUCTFITTING",
    "DUCTSEGMENT",
    "DUCTSILENCER",
    "ELECTRICAPPLIANCE",
    "ELECTRICDISTRIBUTIONBOARD",
    "ENERGYCONVERSIONDEVICE",
    "EVAPORATIVECOOLER",
    "EVAPORATOR",
    "FAN",
    "FILTER",
    "FIRESUPPRESSIONTERMINAL",
    "FLOWCONTROLLER",
    "FLOWFITTING",
    "FLOWINSTRUMENT",
    "FLOWMETER",
    "FLOWMOVINGDEVICE",
    "FLOWSEGMENT",
    "FLOWSTORAGEDEVICE",
    "FLOWTERMINAL",
    "FLOWTREATMENTDEVICE",
    "HEATEXCHANGER",
    "HUMIDIFIER",
    "LAMP",
    "LIGHTFIXTURE",
    "OUTLET",
    "PIPEFITTING",
    "PIPESEGMENT",
    "PROTECTIVEDEVICE",
    "PUMP",
    "SANITARYTERMINAL",
    "SENSOR",
    "SPACEHEATER",
    "SWITCHINGDEVICE",
    "TANK",
    "UNITARYCONTROLELEMENT",
    "UNITARYEQUIPMENT",
    "VALVE",
    "WASTETERMINAL",
];

fn es_instalacion(clase: &str) -> bool {
    INSTALACIONES.contains(&clase)
}

// ------------------------------------------------------------------ lectura

/// Texto STEP: `''` es una comilla, `\X2\hhhh…\X0\` UTF-16, `\X\hh` y
/// `\S\c` Latin-1.
fn decodificar(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        let resto: String = c[i..c.len().min(i + 4)].iter().collect();
        if resto.eq_ignore_ascii_case("\\X2\\") {
            i += 4;
            let mut u = Vec::new();
            while i + 4 <= c.len() && c[i] != '\\' {
                let h: String = c[i..i + 4].iter().collect();
                if let Ok(v) = u16::from_str_radix(&h, 16) {
                    u.push(v);
                }
                i += 4;
            }
            out.push_str(&String::from_utf16_lossy(&u));
            // \X0\
            i += 4;
        } else if resto.len() >= 3 && resto[..3].eq_ignore_ascii_case("\\X\\") && i + 5 <= c.len() {
            let h: String = c[i + 3..i + 5].iter().collect();
            if let Ok(v) = u8::from_str_radix(&h, 16) {
                out.push(char::from(v));
            }
            i += 5;
        } else if resto.len() >= 3 && resto[..3].eq_ignore_ascii_case("\\S\\") && i + 3 < c.len() {
            let v = u32::from(c[i + 3]) + 128;
            out.push(char::from_u32(v).unwrap_or('?'));
            i += 4;
        } else if c[i] == '\'' && c.get(i + 1) == Some(&'\'') {
            out.push('\'');
            i += 2;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

struct Analizador<'a> {
    b: &'a [u8],
    i: usize,
}

impl Analizador<'_> {
    fn blancos(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn ident(&mut self) -> String {
        let ini = self.i;
        while self.i < self.b.len() && (self.b[self.i].is_ascii_alphanumeric() || self.b[self.i] == b'_') {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[ini..self.i]).to_ascii_uppercase()
    }

    fn lista(&mut self) -> Vec<V> {
        // Ya en '('
        self.i += 1;
        let mut v = Vec::new();
        loop {
            self.blancos();
            match self.b.get(self.i) {
                None => break,
                Some(b')') => {
                    self.i += 1;
                    break;
                }
                Some(b',') => self.i += 1,
                Some(_) => {
                    let antes = self.i;
                    v.push(self.valor());
                    if self.i == antes {
                        self.i += 1;
                    }
                }
            }
        }
        v
    }

    fn valor(&mut self) -> V {
        self.blancos();
        let Some(&c) = self.b.get(self.i) else {
            return V::Nulo;
        };
        match c {
            b'$' | b'*' => {
                self.i += 1;
                V::Nulo
            }
            b'#' => {
                self.i += 1;
                let ini = self.i;
                while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                    self.i += 1;
                }
                std::str::from_utf8(&self.b[ini..self.i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map_or(V::Nulo, V::Ref)
            }
            b'\'' => {
                self.i += 1;
                let ini = self.i;
                while self.i < self.b.len() {
                    if self.b[self.i] == b'\'' {
                        if self.b.get(self.i + 1) == Some(&b'\'') {
                            self.i += 2;
                            continue;
                        }
                        break;
                    }
                    self.i += 1;
                }
                let s = String::from_utf8_lossy(&self.b[ini..self.i]).into_owned();
                self.i += 1;
                V::Txt(decodificar(&s))
            }
            b'.' => {
                self.i += 1;
                let ini = self.i;
                while self.i < self.b.len() && self.b[self.i] != b'.' {
                    self.i += 1;
                }
                let s = String::from_utf8_lossy(&self.b[ini..self.i]).to_ascii_uppercase();
                self.i += 1;
                V::Enum(s)
            }
            b'(' => V::Lista(self.lista()),
            b'"' => {
                // Binario: no se usa
                self.i += 1;
                while self.i < self.b.len() && self.b[self.i] != b'"' {
                    self.i += 1;
                }
                self.i += 1;
                V::Nulo
            }
            c if c.is_ascii_digit() || c == b'-' || c == b'+' => {
                let ini = self.i;
                self.i += 1;
                while self.i < self.b.len()
                    && (self.b[self.i].is_ascii_digit() || matches!(self.b[self.i], b'.' | b'E' | b'e' | b'-' | b'+'))
                {
                    self.i += 1;
                }
                std::str::from_utf8(&self.b[ini..self.i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map_or(V::Nulo, V::Num)
            }
            c if c.is_ascii_alphabetic() => {
                let tipo = self.ident();
                self.blancos();
                if self.b.get(self.i) == Some(&b'(') {
                    let mut l = self.lista();
                    let v = if l.len() == 1 { l.remove(0) } else { V::Lista(l) };
                    V::Tipado(tipo, Box::new(v))
                } else {
                    V::Enum(tipo)
                }
            }
            _ => V::Nulo,
        }
    }
}

/// Sentencias de la sección DATA, cortadas por `;` fuera de los textos.
fn sentencias(t: &str) -> Vec<&str> {
    let ini = t.find("DATA;").map_or(0, |i| i + 5);
    let b = t.as_bytes();
    let mut v = Vec::new();
    let mut desde = ini;
    let mut en_texto = false;
    for (i, &c) in b.iter().enumerate().skip(ini) {
        match c {
            b'\'' => en_texto = !en_texto,
            b';' if !en_texto => {
                v.push(t[desde..i].trim());
                desde = i + 1;
            }
            _ => {}
        }
    }
    v
}

fn entidades(t: &str) -> HashMap<u64, Entidad> {
    let mut m = HashMap::new();
    for s in sentencias(t) {
        let Some(resto) = s.strip_prefix('#') else {
            continue;
        };
        let Some((id, cuerpo)) = resto.split_once('=') else {
            continue;
        };
        let Ok(id) = id.trim().parse::<u64>() else {
            continue;
        };
        let mut a = Analizador {
            b: cuerpo.trim().as_bytes(),
            i: 0,
        };
        let tipo = a.ident();
        a.blancos();
        if a.b.get(a.i) != Some(&b'(') {
            continue;
        }
        let args = a.lista();
        m.insert(id, Entidad { tipo, args });
    }
    m
}

fn prefijo(p: &V) -> f64 {
    match p {
        V::Enum(e) => match e.as_str() {
            "MILLI" => 0.001,
            "CENTI" => 0.01,
            "DECI" => 0.1,
            "KILO" => 1000.0,
            _ => 1.0,
        },
        _ => 1.0,
    }
}

/// Nombre sin el id que Revit añade al final («Difusor:600x600:123456»).
fn limpiar_nombre(s: &str) -> String {
    let s = s.trim();
    match s.rsplit_once(':') {
        Some((a, b)) if !b.is_empty() && b.chars().all(|c| c.is_ascii_digit()) => a.trim().to_owned(),
        _ => s.to_owned(),
    }
}

fn clase_bonita(tipo: &str) -> String {
    tipo.strip_prefix("IFC").unwrap_or(tipo).to_owned()
}

const LONGITUD: &[&str] = &["length", "longitud", "largo", "netlength", "grosslength"];
const TAMANO: &[&str] = &["size", "tamaño", "tamano", "dimensión", "dimension"];
const DIAMETRO: &[&str] = &[
    "nominaldiameter",
    "diámetro nominal",
    "diametro nominal",
    "diameter",
    "diámetro",
];
const SUPERFICIE: &[&str] = &["netfloorarea", "grossfloorarea", "area", "área", "superficie"];

fn es(nombre: &str, lista: &[&str]) -> bool {
    lista.contains(&nombre.trim().to_lowercase().as_str())
}

/// Longitud de un segmento sacada de su geometría: la mayor profundidad de
/// los sólidos extruidos de su representación (así exporta Revit conductos y
/// tuberías rectos). `None` si la geometría es de otro tipo (mallas…).
fn profundidad_extrusion(ent: &HashMap<u64, Entidad>, e: &Entidad) -> Option<f64> {
    fn de_item(ent: &HashMap<u64, Entidad>, id: u64, nivel: u8) -> Option<f64> {
        if nivel > 6 {
            return None;
        }
        let it = ent.get(&id)?;
        match it.tipo.as_str() {
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => it.arg(3).num(),
            "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => de_item(ent, *it.arg(1).refs().first()?, nivel + 1),
            "IFCMAPPEDITEM" => {
                let mapa = ent.get(it.arg(0).refs().first()?)?;
                de_rep(ent, *mapa.arg(1).refs().first()?, nivel + 1)
            }
            _ => None,
        }
    }
    fn de_rep(ent: &HashMap<u64, Entidad>, id: u64, nivel: u8) -> Option<f64> {
        let r = ent.get(&id)?;
        r.arg(3)
            .refs()
            .into_iter()
            .filter_map(|i| de_item(ent, i, nivel))
            .reduce(f64::max)
    }
    let forma = ent.get(e.arg(6).refs().first()?)?;
    forma
        .arg(2)
        .refs()
        .into_iter()
        .filter(|r| {
            ent.get(r)
                .and_then(|x| x.arg(1).texto())
                .is_none_or(|t| t.eq_ignore_ascii_case("body"))
        })
        .filter_map(|r| de_rep(ent, r, 0))
        .reduce(f64::max)
}

pub fn leer(ruta: &Path) -> Result<Modelo, String> {
    let b = std::fs::read(ruta).map_err(|e| format!("no se pudo leer {}: {e}", ruta.display()))?;
    if b.starts_with(b"PK") {
        return Err("IFC comprimido (ifcZIP): descomprímalo o expórtelo como .ifc".into());
    }
    leer_texto(&String::from_utf8_lossy(&b))
}

#[allow(clippy::too_many_lines)]
pub fn leer_texto(t: &str) -> Result<Modelo, String> {
    if !t.trim_start().starts_with("ISO-10303-21") {
        return Err("no es un IFC en texto (ISO-10303-21)".into());
    }
    let esquema = t
        .find("FILE_SCHEMA")
        .and_then(|i| {
            let r = &t[i..];
            let a = r.find('\'')? + 1;
            let f = r[a..].find('\'')? + a;
            Some(r[a..f].to_owned())
        })
        .unwrap_or_default();
    let ent = entidades(t);
    // Unidades
    let mut escala = 1.0;
    let mut escala_area = None;
    for e in ent.values().filter(|e| e.tipo == "IFCSIUNIT") {
        match (e.arg(1), e.arg(3)) {
            (V::Enum(u), V::Enum(n)) if u == "LENGTHUNIT" && n == "METRE" => escala = prefijo(e.arg(2)),
            (V::Enum(u), V::Enum(n)) if u == "AREAUNIT" && n == "SQUARE_METRE" => {
                let p = prefijo(e.arg(2));
                escala_area = Some(p * p);
            }
            _ => {}
        }
    }
    let escala_area = escala_area.unwrap_or(escala * escala);
    let nombre_de = |id: u64| ent.get(&id).and_then(|e| e.arg(2).texto()).map(limpiar_nombre);

    // Planta de cada elemento y de cada espacio
    let mut planta: HashMap<u64, String> = HashMap::new();
    let mut padre: HashMap<u64, u64> = HashMap::new();
    let mut tipo_de: HashMap<u64, String> = HashMap::new();
    let mut psets: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut contenidos = Vec::new();
    for e in ent.values() {
        match e.tipo.as_str() {
            "IFCRELCONTAINEDINSPATIALSTRUCTURE" => {
                let s = e.arg(5).refs();
                for r in e.arg(4).refs() {
                    contenidos.push(r);
                    if let Some(&s) = s.first() {
                        padre.insert(r, s);
                    }
                }
            }
            "IFCRELAGGREGATES" => {
                if let Some(&p) = e.arg(4).refs().first() {
                    for r in e.arg(5).refs() {
                        padre.entry(r).or_insert(p);
                    }
                }
            }
            "IFCRELDEFINESBYTYPE" => {
                if let Some(n) = e.arg(5).refs().first().and_then(|&t| nombre_de(t)) {
                    for r in e.arg(4).refs() {
                        tipo_de.insert(r, n.clone());
                    }
                }
            }
            "IFCRELDEFINESBYPROPERTIES" => {
                if let Some(&p) = e.arg(5).refs().first() {
                    for r in e.arg(4).refs() {
                        psets.entry(r).or_default().push(p);
                    }
                }
            }
            _ => {}
        }
    }
    // Sube por la estructura hasta una planta (IfcBuildingStorey).
    let planta_de = |mut id: u64, planta: &mut HashMap<u64, String>| -> String {
        if let Some(p) = planta.get(&id) {
            return p.clone();
        }
        let ini = id;
        for _ in 0..8 {
            let Some(&p) = padre.get(&id) else { break };
            if let Some(e) = ent.get(&p)
                && e.tipo == "IFCBUILDINGSTOREY"
            {
                let n = e.arg(2).texto().unwrap_or("planta sin nombre").trim().to_owned();
                planta.insert(ini, n.clone());
                return n;
            }
            id = p;
        }
        "sin planta".to_owned()
    };
    // Propiedades y cantidades de un objeto (las del tipo también valen).
    let valores = |id: u64| -> Vec<(String, V)> {
        let mut v = Vec::new();
        for p in psets.get(&id).into_iter().flatten() {
            let Some(d) = ent.get(p) else { continue };
            let hijos = match d.tipo.as_str() {
                "IFCPROPERTYSET" => d.arg(4).refs(),
                "IFCELEMENTQUANTITY" => d.arg(5).refs(),
                _ => Vec::new(),
            };
            for h in hijos {
                let Some(x) = ent.get(&h) else { continue };
                let Some(n) = x.arg(0).texto() else { continue };
                let val = match x.tipo.as_str() {
                    "IFCPROPERTYSINGLEVALUE" => x.arg(2).clone(),
                    "IFCQUANTITYLENGTH" | "IFCQUANTITYAREA" => x.arg(3).clone(),
                    _ => continue,
                };
                v.push((n.to_owned(), val));
            }
        }
        v
    };

    let mut ids: Vec<u64> = ent
        .iter()
        .filter(|(_, e)| es_instalacion(e.tipo.strip_prefix("IFC").unwrap_or(&e.tipo)))
        .map(|(id, _)| *id)
        .collect();
    ids.extend(contenidos.iter().copied());
    ids.sort_unstable();
    ids.dedup();
    let mut elementos = Vec::new();
    for id in ids {
        let Some(e) = ent.get(&id) else { continue };
        if e.tipo.starts_with("IFCREL") || e.tipo == "IFCSPACE" {
            continue;
        }
        // Solo objetos con GlobalId
        if !matches!(e.arg(0), V::Txt(_)) {
            continue;
        }
        let nombre = tipo_de
            .get(&id)
            .cloned()
            .or_else(|| e.arg(4).texto().map(limpiar_nombre))
            .or_else(|| e.arg(2).texto().map(limpiar_nombre))
            .unwrap_or_default();
        let vals = valores(id);
        let longitud = vals
            .iter()
            .find(|(n, v)| es(n, LONGITUD) && v.num().is_some())
            .and_then(|(_, v)| v.num())
            .or_else(|| {
                e.tipo
                    .ends_with("SEGMENT")
                    .then(|| profundidad_extrusion(&ent, e))
                    .flatten()
            })
            .map(|l| l * escala);
        let tamano = vals
            .iter()
            .find(|(n, v)| es(n, TAMANO) && v.texto().is_some())
            .and_then(|(_, v)| v.texto().map(|s| s.trim().to_owned()))
            .or_else(|| {
                vals.iter()
                    .find(|(n, v)| es(n, DIAMETRO) && v.num().is_some())
                    .and_then(|(_, v)| v.num())
                    .map(|d| format!("Ø{:.0} mm", d * escala * 1000.0))
            })
            .unwrap_or_default();
        elementos.push(Elemento {
            clase: clase_bonita(&e.tipo),
            nombre,
            planta: planta_de(id, &mut planta),
            tamano,
            longitud,
        });
    }
    let mut espacios = Vec::new();
    let mut sids: Vec<u64> = ent
        .iter()
        .filter(|(_, e)| e.tipo == "IFCSPACE")
        .map(|(id, _)| *id)
        .collect();
    sids.sort_unstable();
    for id in sids {
        let e = &ent[&id];
        let num = e.arg(2).texto().unwrap_or("").trim();
        let largo = e.arg(7).texto().unwrap_or("").trim();
        let nombre = match (num.is_empty(), largo.is_empty()) {
            (false, false) => format!("{num} {largo}"),
            (true, false) => largo.to_owned(),
            _ => num.to_owned(),
        };
        let superficie = valores(id)
            .iter()
            .find(|(n, v)| es(n, SUPERFICIE) && v.num().is_some())
            .and_then(|(_, v)| v.num())
            .map(|a| a * escala_area);
        espacios.push(Espacio {
            nombre,
            planta: planta_de(id, &mut planta),
            superficie,
        });
    }
    Ok(Modelo {
        esquema,
        elementos,
        espacios,
        escala,
    })
}

// ------------------------------------------------------------------ resumen

/// Clase, tipo, tamaño y planta.
type Grupo<'a> = (&'a str, &'a str, &'a str, &'a str);
/// Uds, metros y uds con longitud.
type Suma = (usize, f64, usize);

pub fn resumen(m: &Modelo, nombre: &str, filtro: Option<&str>) -> String {
    let filtro = filtro.map(str::to_lowercase).filter(|f| !f.trim().is_empty());
    let pasa = |e: &Elemento| {
        filtro.as_ref().is_none_or(|f| {
            e.clase.to_lowercase().contains(f)
                || e.nombre.to_lowercase().contains(f)
                || e.tamano.to_lowercase().contains(f)
        })
    };
    let inst: Vec<&Elemento> = m
        .elementos
        .iter()
        .filter(|e| es_instalacion(&e.clase.to_uppercase()) && pasa(e))
        .collect();
    let otros: Vec<&Elemento> = m
        .elementos
        .iter()
        .filter(|e| !es_instalacion(&e.clase.to_uppercase()) && pasa(e))
        .collect();
    let mut t = format!(
        "Modelo IFC {nombre} ({}): {} elementos de instalaciones, {} de otro tipo, {} espacios. Unidad de longitud: {}.\n",
        if m.esquema.is_empty() {
            "esquema sin indicar"
        } else {
            &m.esquema
        },
        inst.len(),
        otros.len(),
        m.espacios.len(),
        match m.escala {
            x if (x - 0.001).abs() < 1e-9 => "mm",
            x if (x - 0.01).abs() < 1e-9 => "cm",
            x if (x - 1.0).abs() < 1e-9 => "m",
            _ => "otra",
        }
    );
    // Instalaciones: clase, tipo, tamaño y planta, con uds y metros
    let mut g: BTreeMap<Grupo<'_>, Suma> = BTreeMap::new();
    for e in &inst {
        let x = g
            .entry((
                e.clase.as_str(),
                e.nombre.as_str(),
                e.tamano.as_str(),
                e.planta.as_str(),
            ))
            .or_default();
        x.0 += 1;
        if let Some(l) = e.longitud {
            x.1 += l;
            x.2 += 1;
        }
    }
    if g.is_empty() {
        t.push_str("\nNo hay elementos de instalaciones (o no coinciden con el filtro).\n");
    } else {
        t.push_str("\n| Clase IFC | Tipo | Tamaño | Planta | Uds | m |\n|---|---|---|---|---:|---:|\n");
        for (n, ((c, ti, ta, p), (u, l, con))) in g.iter().enumerate() {
            if n >= 150 {
                let _ = writeln!(t, "(y {} filas más; use «bloque» para filtrar)", g.len() - 150);
                break;
            }
            let m = if *con == 0 {
                String::new()
            } else if con < u {
                format!("{l:.1} ({con} de {u} con longitud)")
            } else {
                format!("{l:.1}")
            };
            let _ = writeln!(t, "| {c} | {ti} | {ta} | {p} | {u} | {m} |");
        }
        let sin = inst
            .iter()
            .filter(|e| e.clase.ends_with("SEGMENT") && e.longitud.is_none())
            .count();
        if sin > 0 {
            let _ = writeln!(
                t,
                "\n{sin} segmentos sin longitud en sus propiedades: se cuentan pero no se miden."
            );
        }
    }
    // Resto (arquitectura, estructura…), solo por clase
    if !otros.is_empty() {
        let mut c: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &otros {
            *c.entry(e.clase.as_str()).or_default() += 1;
        }
        t.push_str("\nOtros elementos por clase: ");
        let v: Vec<String> = c.iter().map(|(k, n)| format!("{k} {n}")).collect();
        t.push_str(&v.join(", "));
        t.push('\n');
    }
    // Espacios por planta
    if !m.espacios.is_empty() {
        let mut p: BTreeMap<&str, (usize, f64, usize)> = BTreeMap::new();
        for s in &m.espacios {
            let x = p.entry(s.planta.as_str()).or_default();
            x.0 += 1;
            if let Some(a) = s.superficie {
                x.1 += a;
                x.2 += 1;
            }
        }
        t.push_str("\n| Planta | Espacios | m² |\n|---|---:|---:|\n");
        for (pl, (n, a, con)) in &p {
            let _ = writeln!(
                t,
                "| {pl} | {n} | {} |",
                if *con == 0 { String::new() } else { format!("{a:.1}") }
            );
        }
        if filtro.is_some() {
            for s in m
                .espacios
                .iter()
                .filter(|s| pasa_espacio(s, filtro.as_deref()))
                .take(100)
            {
                let _ = writeln!(
                    t,
                    "- {} [{}]{}",
                    s.nombre,
                    s.planta,
                    s.superficie.map(|a| format!(": {a:.1} m²")).unwrap_or_default()
                );
            }
        }
    }
    t.push_str(
        "\nCompare uds y metros con la medición del presupuesto; los nombres de tipo vienen del modelo \
         (familia y tipo en Revit). Con «bloque» = texto se filtra por clase, tipo o tamaño.",
    );
    t
}

fn pasa_espacio(s: &Espacio, f: Option<&str>) -> bool {
    f.is_some_and(|f| s.nombre.to_lowercase().contains(f) || s.planta.to_lowercase().contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modelo sintético, IFC4 en mm.
    const IFC: &str = r"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('ViewDefinition [CoordinationView]'),'2;1');
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);
#2=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);
#10=IFCBUILDINGSTOREY('0a',$,'Planta baja',$,$,$,$,$,.ELEMENT.,0.);
#11=IFCBUILDINGSTOREY('0b',$,'S\X2\00F3\X0\tano',$,$,$,$,$,.ELEMENT.,-3000.);
#20=IFCSENSOR('1a',$,'Detector \X\F3ptico:Std:101',$,$,$,$,$,.SMOKESENSOR.);
#21=IFCSENSOR('1b',$,'Detector \X\F3ptico:Std:102',$,$,$,$,$,.SMOKESENSOR.);
#22=IFCFIRESUPPRESSIONTERMINAL('1c',$,'BIE 25:Armario:103',$,$,$,$,$,.FIREHYDRANT.);
#23=IFCDUCTSEGMENT('1d',$,'Conducto rectangular:Chapa:104',$,$,$,$,$,.RIGIDSEGMENT.);
#24=IFCDUCTSEGMENT('1e',$,'Conducto rectangular:Chapa:105',$,$,$,$,$,.RIGIDSEGMENT.);
#25=IFCWALL('1f',$,'Muro:200',$,$,$,$,$,$);
#26=IFCPIPESEGMENT('1g',$,'Tuber\X\EDa acero:DN50:106',$,$,$,#60,$,.RIGIDSEGMENT.);
#60=IFCPRODUCTDEFINITIONSHAPE($,$,(#61));
#61=IFCSHAPEREPRESENTATION(#99,'Body','SweptSolid',(#62));
#62=IFCEXTRUDEDAREASOLID(#98,$,$,6000.);
#30=IFCRELCONTAINEDINSPATIALSTRUCTURE('2a',$,$,$,(#20,#22,#23,#24,#25,#26),#10);
#31=IFCRELCONTAINEDINSPATIALSTRUCTURE('2b',$,$,$,(#21),#11);
#40=IFCPROPERTYSINGLEVALUE('Length',$,IFCLENGTHMEASURE(2500.),$);
#41=IFCPROPERTYSINGLEVALUE('Size',$,IFCLABEL('400x300'),$);
#42=IFCPROPERTYSET('3a',$,'Dimensions',$,(#40,#41));
#43=IFCRELDEFINESBYPROPERTIES('3b',$,$,$,(#23,#24),#42);
#50=IFCSPACE('4a',$,'01',$,$,$,$,'Nave; templo',.ELEMENT.,.INTERNAL.,$);
#51=IFCQUANTITYAREA('NetFloorArea',$,$,850.5,$);
#52=IFCELEMENTQUANTITY('4b',$,'BaseQuantities',$,$,(#51));
#53=IFCRELDEFINESBYPROPERTIES('4c',$,$,$,(#50),#52);
#54=IFCRELAGGREGATES('4d',$,$,$,#10,(#50));
ENDSEC;
END-ISO-10303-21;
";

    #[test]
    fn lee_elementos_plantas_y_longitudes() {
        let m = leer_texto(IFC).unwrap();
        assert_eq!(m.esquema, "IFC4");
        assert!((m.escala - 0.001).abs() < 1e-12);
        let det: Vec<_> = m.elementos.iter().filter(|e| e.clase == "SENSOR").collect();
        assert_eq!(det.len(), 2);
        assert_eq!(det[0].nombre, "Detector óptico:Std");
        assert!(det.iter().any(|e| e.planta == "Sótano"));
        let cond: Vec<_> = m.elementos.iter().filter(|e| e.clase == "DUCTSEGMENT").collect();
        assert_eq!(cond.len(), 2);
        assert_eq!(cond[0].tamano, "400x300");
        assert!((cond.iter().filter_map(|e| e.longitud).sum::<f64>() - 5.0).abs() < 1e-9);
        // Tubería medida por su extrusión
        let tub = m.elementos.iter().find(|e| e.clase == "PIPESEGMENT").unwrap();
        assert_eq!(tub.nombre, "Tubería acero:DN50");
        assert!((tub.longitud.unwrap() - 6.0).abs() < 1e-9);
        // El muro cuenta como «otro»
        assert!(m.elementos.iter().any(|e| e.clase == "WALL"));
        assert_eq!(m.espacios.len(), 1);
        assert_eq!(m.espacios[0].nombre, "01 Nave; templo");
        assert_eq!(m.espacios[0].planta, "Planta baja");
        assert!((m.espacios[0].superficie.unwrap() - 850.5).abs() < 1e-9);
    }

    #[test]
    fn resumen_y_filtro() {
        let m = leer_texto(IFC).unwrap();
        let t = resumen(&m, "prueba.ifc", None);
        assert!(t.contains("| DUCTSEGMENT | Conducto rectangular:Chapa | 400x300 | Planta baja | 2 | 5.0 |"));
        assert!(t.contains("| FIRESUPPRESSIONTERMINAL | BIE 25:Armario |"));
        assert!(t.contains("WALL 1"));
        assert!(t.contains("| Planta baja | 1 | 850.5 |"));
        let f = resumen(&m, "prueba.ifc", Some("detector"));
        assert!(f.contains("SENSOR"));
        assert!(!f.contains("DUCTSEGMENT |"));
    }

    #[test]
    fn rechaza_lo_que_no_es_ifc() {
        assert!(leer_texto("0\nSECTION\n").is_err());
    }
}
