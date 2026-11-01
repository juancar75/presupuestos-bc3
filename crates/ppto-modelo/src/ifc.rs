// SPDX-License-Identifier: GPL-3.0-or-later
//! Lector de IFC (STEP, ISO 10303-21) para IFC2x3, IFC4 e IFC4.3.
//!
//! Solo lee lo que hace falta para medir: elementos de instalaciones, su
//! tipo, planta, sistema, cantidades (`IfcElementQuantity`) y propiedades
//! habituales de tamaño y longitud (también las que exporta Revit en
//! español).

use crate::{Clase, Elemento, ErrorModelo, Modelo, limpiar_tipo, normalizar_medida};
use ppto_core::{Decimal, redondear};
use std::collections::HashMap;
use std::str::FromStr;

/// Valor de un atributo STEP.
#[derive(Debug, Clone, PartialEq)]
pub enum Valor {
    Ref(u64),
    Texto(String),
    Num(Decimal),
    Enum(String),
    Lista(Vec<Valor>),
    /// `IFCLENGTHMEASURE(0.5)`.
    Tipado(String, Box<Valor>),
    Nulo,
}

impl Valor {
    fn texto(&self) -> Option<&str> {
        match self {
            Self::Texto(t) => Some(t),
            Self::Tipado(_, v) => v.texto(),
            _ => None,
        }
    }

    fn referencia(&self) -> Option<u64> {
        match self {
            Self::Ref(r) => Some(*r),
            _ => None,
        }
    }

    fn referencias(&self) -> Vec<u64> {
        match self {
            Self::Lista(l) => l.iter().filter_map(Valor::referencia).collect(),
            Self::Ref(r) => vec![*r],
            _ => Vec::new(),
        }
    }

    fn numero(&self) -> Option<Decimal> {
        match self {
            Self::Num(n) => Some(*n),
            Self::Tipado(_, v) => v.numero(),
            _ => None,
        }
    }

    fn tipo_medida(&self) -> Option<&str> {
        match self {
            Self::Tipado(t, _) => Some(t),
            _ => None,
        }
    }
}

/// Entidad: nombre (`IFCPIPESEGMENT`) y atributos.
#[derive(Debug, Clone, PartialEq)]
pub struct Entidad {
    pub nombre: String,
    pub attrs: Vec<Valor>,
}

impl Entidad {
    fn attr(&self, i: usize) -> &Valor {
        self.attrs.get(i).unwrap_or(&Valor::Nulo)
    }

    fn texto(&self, i: usize) -> String {
        self.attr(i).texto().unwrap_or_default().trim().to_owned()
    }
}

// ------------------------------------------------------------------ STEP

struct Lector<'a> {
    b: &'a [u8],
    i: usize,
}

impl Lector<'_> {
    fn espacios(&mut self) {
        loop {
            while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            // Comentarios /* … */
            if self.b[self.i..].starts_with(b"/*") {
                match find(&self.b[self.i + 2..], b"*/") {
                    Some(f) => self.i += f + 4,
                    None => self.i = self.b.len(),
                }
            } else {
                return;
            }
        }
    }

    fn mirar(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn valor(&mut self) -> Result<Valor, String> {
        self.espacios();
        let c = self.mirar().ok_or("fin de fichero inesperado")?;
        Ok(match c {
            b'$' | b'*' => {
                self.i += 1;
                Valor::Nulo
            }
            b'#' => {
                self.i += 1;
                let ini = self.i;
                while self.mirar().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                Valor::Ref(
                    std::str::from_utf8(&self.b[ini..self.i])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .ok_or("referencia no válida")?,
                )
            }
            b'\'' => Valor::Texto(self.cadena()?),
            b'.' => {
                self.i += 1;
                let ini = self.i;
                while self.mirar().is_some_and(|c| c != b'.') {
                    self.i += 1;
                }
                let t = String::from_utf8_lossy(&self.b[ini..self.i]).into_owned();
                self.i += 1;
                Valor::Enum(t)
            }
            b'(' => Valor::Lista(self.lista()?),
            b'"' => {
                // Binario: se ignora
                self.i += 1;
                while self.mirar().is_some_and(|c| c != b'"') {
                    self.i += 1;
                }
                self.i += 1;
                Valor::Nulo
            }
            c if c.is_ascii_alphabetic() => {
                let ini = self.i;
                while self.mirar().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                    self.i += 1;
                }
                let t = String::from_utf8_lossy(&self.b[ini..self.i]).to_ascii_uppercase();
                self.espacios();
                let l = self.lista()?;
                Valor::Tipado(t, Box::new(l.into_iter().next().unwrap_or(Valor::Nulo)))
            }
            _ => {
                let ini = self.i;
                while self
                    .mirar()
                    .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+' | b'E' | b'e'))
                {
                    self.i += 1;
                }
                if ini == self.i {
                    return Err(format!("carácter inesperado «{}»", c as char));
                }
                let t = std::str::from_utf8(&self.b[ini..self.i]).unwrap_or_default();
                Valor::Num(numero(t).ok_or_else(|| format!("número no válido «{t}»"))?)
            }
        })
    }

    fn lista(&mut self) -> Result<Vec<Valor>, String> {
        self.espacios();
        if self.mirar() != Some(b'(') {
            return Err("se esperaba «(»".into());
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            self.espacios();
            match self.mirar() {
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b',') => self.i += 1,
                Some(_) => out.push(self.valor()?),
                None => return Err("lista sin cerrar".into()),
            }
        }
    }

    fn cadena(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut bytes = Vec::new();
        loop {
            match self.mirar() {
                None => return Err("texto sin cerrar".into()),
                Some(b'\'') if self.b.get(self.i + 1) == Some(&b'\'') => {
                    bytes.push(b'\'');
                    self.i += 2;
                }
                Some(b'\'') => {
                    self.i += 1;
                    break;
                }
                Some(c) => {
                    bytes.push(c);
                    self.i += 1;
                }
            }
        }
        Ok(decodificar(&String::from_utf8_lossy(&bytes)))
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Número STEP («0.032», «1.E-3», «12.»).
fn numero(t: &str) -> Option<Decimal> {
    let t = t.trim();
    let (m, e) = match t.find(['E', 'e']) {
        Some(i) => (&t[..i], Some(&t[i + 1..])),
        None => (t, None),
    };
    let m = if m.ends_with('.') {
        format!("{m}0")
    } else {
        m.to_owned()
    };
    match e {
        None => Decimal::from_str(&m).ok(),
        Some(e) => {
            let e: i32 = e.parse().ok()?;
            let base = Decimal::from_str(&m).ok()?;
            let mut r = base;
            if e >= 0 {
                for _ in 0..e.min(28) {
                    r = r.checked_mul(Decimal::TEN)?;
                }
            } else {
                for _ in 0..(-e).min(28) {
                    r /= Decimal::TEN;
                }
            }
            Some(r)
        }
    }
}

/// Escapes de texto STEP: `\X2\00F1\X0\`, `\X\F1`, `\S\q`.
pub fn decodificar(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut r = s;
    while let Some(i) = r.find('\\') {
        out.push_str(&r[..i]);
        r = &r[i..];
        if let Some(x) = r.strip_prefix("\\X2\\") {
            let fin = x.find("\\X0\\").unwrap_or(x.len());
            let hex = &x[..fin];
            let unidades: Vec<u16> = hex
                .as_bytes()
                .chunks(4)
                .filter_map(|c| u16::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
                .collect();
            out.push_str(&String::from_utf16_lossy(&unidades));
            r = x.get(fin + 4..).unwrap_or("");
        } else if let Some(x) = r.strip_prefix("\\X4\\") {
            let fin = x.find("\\X0\\").unwrap_or(x.len());
            for c in x.as_bytes()[..fin].chunks(8) {
                if let Some(ch) = std::str::from_utf8(c)
                    .ok()
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .and_then(char::from_u32)
                {
                    out.push(ch);
                }
            }
            r = x.get(fin + 4..).unwrap_or("");
        } else if let Some(x) = r.strip_prefix("\\X\\") {
            if let Some(b) = x.get(..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(char::from(b));
                r = &x[2..];
            } else {
                out.push('\\');
                r = &r[1..];
            }
        } else if let Some(x) = r.strip_prefix("\\S\\") {
            if let Some(c) = x.chars().next() {
                out.push(char::from_u32(c as u32 + 128).unwrap_or(c));
                r = &x[c.len_utf8()..];
            } else {
                r = x;
            }
        } else if let Some(x) = r.strip_prefix("\\\\") {
            out.push('\\');
            r = x;
        } else if r.starts_with("\\P") && r.len() >= 4 && r.as_bytes()[3] == b'\\' {
            r = &r[4..];
        } else {
            out.push('\\');
            r = &r[1..];
        }
    }
    out.push_str(r);
    out
}

/// Lee las entidades de la sección DATA.
pub fn entidades(bytes: &[u8]) -> Result<HashMap<u64, Entidad>, ErrorModelo> {
    let ini = find(bytes, b"DATA;").ok_or_else(|| ErrorModelo::Ifc("falta la sección DATA".into()))? + 5;
    let mut l = Lector { b: bytes, i: ini };
    let mut out = HashMap::new();
    loop {
        l.espacios();
        match l.mirar() {
            Some(b'#') => {}
            _ => break, // ENDSEC
        }
        let id = match l.valor().map_err(ErrorModelo::Ifc)? {
            Valor::Ref(r) => r,
            _ => return Err(ErrorModelo::Ifc("se esperaba #id".into())),
        };
        l.espacios();
        if l.mirar() != Some(b'=') {
            return Err(ErrorModelo::Ifc(format!("se esperaba «=» en #{id}")));
        }
        l.i += 1;
        l.espacios();
        let ini_n = l.i;
        while l.mirar().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            l.i += 1;
        }
        let nombre = String::from_utf8_lossy(&bytes[ini_n..l.i]).to_ascii_uppercase();
        let attrs = l.lista().map_err(|e| ErrorModelo::Ifc(format!("#{id}: {e}")))?;
        l.espacios();
        if l.mirar() == Some(b';') {
            l.i += 1;
        }
        out.insert(id, Entidad { nombre, attrs });
    }
    Ok(out)
}

// ------------------------------------------------------------------ modelo

const NOMBRES_LONGITUD: [&str; 6] = [
    "length",
    "longitud",
    "largo",
    "netlength",
    "grosslength",
    "longitud de corte",
];
const NOMBRES_MEDIDA: [&str; 13] = [
    "size",
    "tamaño",
    "tamano",
    "nominal size",
    "nominaldiameter",
    "nominal diameter",
    "diámetro nominal",
    "diametro nominal",
    "diámetro",
    "diametro",
    "diameter",
    "outside diameter",
    "diámetro exterior",
];
const NOMBRES_SISTEMA: [&str; 6] = [
    "system name",
    "nombre de sistema",
    "nombre del sistema",
    "sistema",
    "system classification",
    "clasificación de sistema",
];
const NOMBRES_PLANTA: [&str; 4] = ["level", "nivel", "reference level", "nivel de referencia"];

#[derive(Default)]
struct Datos {
    tipo: String,
    clase_tipo: Option<Clase>,
    planta: String,
    sistema: String,
    longitud: Option<Decimal>,
    medida: String,
    props: Vec<(String, Valor)>,
}

/// Factor a metros de la unidad de longitud del proyecto.
fn factor_longitud(e: &HashMap<u64, Entidad>) -> Decimal {
    for x in e.values() {
        if x.nombre == "IFCSIUNIT" && matches!(x.attr(1), Valor::Enum(u) if u == "LENGTHUNIT") {
            return match x.attr(2) {
                Valor::Enum(p) if p == "MILLI" => Decimal::new(1, 3),
                Valor::Enum(p) if p == "CENTI" => Decimal::new(1, 2),
                Valor::Enum(p) if p == "DECI" => Decimal::new(1, 1),
                Valor::Enum(p) if p == "KILO" => Decimal::from(1000),
                _ => Decimal::ONE,
            };
        }
    }
    for x in e.values() {
        if x.nombre == "IFCCONVERSIONBASEDUNIT" && matches!(x.attr(1), Valor::Enum(u) if u == "LENGTHUNIT") {
            let n = x.texto(2).to_lowercase();
            if n.contains("inch") {
                return Decimal::new(254, 4);
            }
            if n.contains("foot") || n.contains("feet") {
                return Decimal::new(3048, 4);
            }
        }
    }
    Decimal::ONE
}

fn es(n: &str, lista: &[&str]) -> bool {
    let n = n.trim().to_lowercase();
    lista.contains(&n.as_str())
}

/// Lee un IFC en memoria.
pub fn leer(bytes: &[u8]) -> Result<Modelo, ErrorModelo> {
    let e = entidades(bytes)?;
    let f = factor_longitud(&e);
    let mut m = Modelo::default();
    if let Some(p) = e.values().find(|x| x.nombre == "IFCPROJECT") {
        m.nombre = [p.texto(2), p.texto(4)]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or_default();
    }
    // Elementos de instalaciones
    let mut ids: Vec<u64> = Vec::new();
    for (id, x) in &e {
        if Clase::de_ifc(&x.nombre).is_some() && !x.nombre.ends_with("TYPE") {
            ids.push(*id);
        } else if [
            "IFCWALL",
            "IFCWALLSTANDARDCASE",
            "IFCSLAB",
            "IFCBEAM",
            "IFCCOLUMN",
            "IFCDOOR",
            "IFCWINDOW",
            "IFCSTAIR",
            "IFCROOF",
            "IFCCOVERING",
            "IFCRAILING",
            "IFCPLATE",
            "IFCMEMBER",
            "IFCCURTAINWALL",
            "IFCFOOTING",
        ]
        .contains(&x.nombre.as_str())
        {
            m.ignorados += 1;
        }
    }
    ids.sort_unstable();
    let mut datos: HashMap<u64, Datos> = ids.iter().map(|i| (*i, Datos::default())).collect();
    // Plantas de los espacios (elementos colocados en un IfcSpace)
    let mut padre_espacial: HashMap<u64, u64> = HashMap::new();
    for x in e.values() {
        if x.nombre == "IFCRELAGGREGATES"
            && let Some(p) = x.attr(4).referencia()
        {
            for h in x.attr(5).referencias() {
                padre_espacial.insert(h, p);
            }
        }
    }
    let planta_de = |mut id: u64| -> String {
        for _ in 0..6 {
            let Some(x) = e.get(&id) else { break };
            if x.nombre == "IFCBUILDINGSTOREY" {
                let n = x.texto(2);
                return if n.is_empty() { x.texto(7) } else { n };
            }
            match padre_espacial.get(&id) {
                Some(p) => id = *p,
                None => break,
            }
        }
        String::new()
    };
    let longitud = |v: &Valor| -> Option<Decimal> {
        let n = v.numero()?;
        // Las medidas IFCLENGTHMEASURE van en la unidad del proyecto
        Some(redondear(n * f, 4))
    };
    for x in e.values() {
        match x.nombre.as_str() {
            "IFCRELDEFINESBYTYPE" => {
                let Some(t) = x.attr(5).referencia().and_then(|r| e.get(&r)) else {
                    continue;
                };
                let nombre = limpiar_tipo(&t.texto(2));
                let clase = Clase::de_ifc(&t.nombre);
                // Propiedades del tipo (HasPropertySets)
                let props: Vec<(String, Valor)> = t
                    .attr(5)
                    .referencias()
                    .into_iter()
                    .filter_map(|r| e.get(&r))
                    .flat_map(|ps| propiedades(&e, ps))
                    .collect();
                for o in x.attr(4).referencias() {
                    if let Some(d) = datos.get_mut(&o) {
                        d.tipo.clone_from(&nombre);
                        d.clase_tipo = clase;
                        d.props.extend(props.iter().cloned());
                    }
                }
            }
            "IFCRELCONTAINEDINSPATIALSTRUCTURE" => {
                let Some(s) = x.attr(5).referencia() else { continue };
                let planta = planta_de(s);
                for o in x.attr(4).referencias() {
                    if let Some(d) = datos.get_mut(&o) {
                        d.planta.clone_from(&planta);
                    }
                }
            }
            "IFCRELASSIGNSTOGROUP" => {
                let Some(g) = x.attr(6).referencia().and_then(|r| e.get(&r)) else {
                    continue;
                };
                if !matches!(
                    g.nombre.as_str(),
                    "IFCSYSTEM" | "IFCDISTRIBUTIONSYSTEM" | "IFCBUILDINGSYSTEM" | "IFCDISTRIBUTIONCIRCUIT"
                ) {
                    continue;
                }
                let n = g.texto(2);
                for o in x.attr(4).referencias() {
                    if let Some(d) = datos.get_mut(&o)
                        && d.sistema.is_empty()
                    {
                        d.sistema.clone_from(&n);
                    }
                }
            }
            "IFCRELDEFINESBYPROPERTIES" => {
                let Some(pd) = x.attr(5).referencia().and_then(|r| e.get(&r)) else {
                    continue;
                };
                let props = propiedades(&e, pd);
                for o in x.attr(4).referencias() {
                    if let Some(d) = datos.get_mut(&o) {
                        d.props.extend(props.iter().cloned());
                    }
                }
            }
            _ => {}
        }
    }
    let mut sin_longitud = 0usize;
    for id in ids {
        let x = &e[&id];
        let Some(d) = datos.remove(&id) else { continue };
        let mut clase = Clase::de_ifc(&x.nombre).unwrap_or(Clase::Otros);
        if clase.generica()
            && let Some(c) = d.clase_tipo
            && c != Clase::Otros
        {
            clase = c;
        }
        let mut d = d;
        for (n, v) in &d.props {
            if es(n, &NOMBRES_LONGITUD) && d.longitud.is_none() {
                d.longitud = longitud(v).filter(|l| *l > Decimal::ZERO);
            } else if es(n, &NOMBRES_MEDIDA) && d.medida.is_empty() {
                d.medida = match (v.tipo_medida(), v.numero()) {
                    (Some(t), Some(n)) if t.contains("LENGTH") || t.contains("POSITIVELENGTH") => {
                        format!("Ø{}", redondear(n * f * Decimal::from(1000), 0))
                    }
                    _ => normalizar_medida(v.texto().unwrap_or_default()),
                };
            } else if es(n, &NOMBRES_SISTEMA) && d.sistema.is_empty() {
                d.sistema = v.texto().unwrap_or_default().trim().to_owned();
            } else if es(n, &NOMBRES_PLANTA) && d.planta.is_empty() {
                d.planta = v.texto().unwrap_or_default().trim().to_owned();
            }
        }
        let tipo = if d.tipo.is_empty() {
            limpiar_tipo(
                &[x.texto(4), x.texto(2)]
                    .into_iter()
                    .find(|t| !t.is_empty())
                    .unwrap_or_default(),
            )
        } else {
            d.tipo
        };
        if clase.lineal() && d.longitud.is_none() {
            sin_longitud += 1;
        }
        m.elementos.push(Elemento {
            clase,
            tipo,
            medida: d.medida,
            planta: d.planta,
            sistema: d.sistema,
            longitud: d.longitud,
            aproximada: false,
        });
    }
    if sin_longitud > 0 {
        m.avisos.push(format!(
            "{sin_longitud} tramo(s) sin longitud en el IFC: al exportar, active las cantidades base (Base Quantities)."
        ));
    }
    if m.elementos.is_empty() {
        m.avisos
            .push("El IFC no tiene elementos de instalaciones (tuberías, conductos, equipos…).".into());
    }
    Ok(m)
}

/// Propiedades y cantidades de un IfcPropertySet o IfcElementQuantity.
fn propiedades(e: &HashMap<u64, Entidad>, pd: &Entidad) -> Vec<(String, Valor)> {
    let lista = match pd.nombre.as_str() {
        "IFCPROPERTYSET" => pd.attr(4),
        "IFCELEMENTQUANTITY" => pd.attr(5),
        _ => return Vec::new(),
    };
    lista
        .referencias()
        .into_iter()
        .filter_map(|r| e.get(&r))
        .filter_map(|p| match p.nombre.as_str() {
            "IFCPROPERTYSINGLEVALUE" => Some((p.texto(0), p.attr(2).clone())),
            "IFCQUANTITYLENGTH" => Some((
                p.texto(0),
                Valor::Tipado("IFCLENGTHMEASURE".into(), Box::new(p.attr(3).clone())),
            )),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn numeros_y_textos() {
        assert_eq!(numero("0.032"), Some(dec!(0.032)));
        assert_eq!(numero("1.E-3"), Some(dec!(0.001)));
        assert_eq!(numero("12."), Some(dec!(12)));
        assert_eq!(numero("-2.5E2"), Some(dec!(-250)));
        assert_eq!(decodificar("Tama\\X2\\00F1\\X0\\o"), "Tamaño");
        assert_eq!(decodificar("Di\\X\\E1metro"), "Diámetro");
        assert_eq!(decodificar("C\\S\\iodo"), "C\u{e9}odo");
    }
}
