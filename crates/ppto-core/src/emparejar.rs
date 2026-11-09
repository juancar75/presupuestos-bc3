// SPDX-License-Identifier: GPL-3.0-or-later
//! Emparejar las partidas de una medición (p. ej. un presupuesto ciego) con
//! las partidas de una base de precios.
//!
//! Para cada partida sin precio se puntúan las partidas de la base:
//!
//! - **Datos técnicos** (pesan el doble): modelos («CHGT/4-560-6»), medidas
//!   («400x300»), diámetros («Ø125» → `d125`, «DN50» → `dn50`), potencias
//!   («0,55 kW» → `0.55kw`) y números de dos o más cifras.
//! - **Palabras** del resumen, sin tildes, sin palabras vacías y sin plural.
//! - La **unidad** tiene que ser la misma (m = ml, m2 = m²); si no, se descarta.
//!
//! La puntuación (0–100) es la parte de los datos de la medición que aparece
//! en la partida de la base. Si la medición trae datos técnicos y ninguno
//! coincide, se queda como dudosa como mucho. Nada se asigna solo: el técnico
//! confirma cada pareja, y las confirmadas se recuerdan (ver
//! [`clave`]) para que la próxima vez salgan ya emparejadas.

use crate::concepto::Naturaleza;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use std::collections::BTreeSet;

/// Puntos a partir de los cuales la pareja se da por buena (verde).
pub const SEGURO: u32 = 70;
/// Puntos a partir de los cuales hay candidato, pero dudoso (naranja).
pub const DUDOSO: u32 = 45;

/// Confianza de un emparejamiento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Nivel {
    Ninguno,
    Dudoso,
    Seguro,
}

impl Nivel {
    pub fn de(puntos: u32) -> Self {
        if puntos >= SEGURO {
            Self::Seguro
        } else if puntos >= DUDOSO {
            Self::Dudoso
        } else {
            Self::Ninguno
        }
    }
}

/// Partida de la base candidata para una partida de la medición.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub precio: Decimal,
    /// 0–100.
    pub puntos: u32,
    /// Viene de una pareja confirmada antes.
    pub recordado: bool,
    /// Otra partida distinta tiene los mismos puntos: hay que elegir.
    pub empate: bool,
}

impl Candidato {
    pub fn nivel(&self) -> Nivel {
        match Nivel::de(self.puntos) {
            Nivel::Seguro if self.empate && !self.recordado => Nivel::Dudoso,
            n => n,
        }
    }
}

const VACIAS: &[&str] = &[
    "de",
    "del",
    "la",
    "el",
    "los",
    "las",
    "con",
    "para",
    "por",
    "una",
    "uno",
    "unos",
    "unas",
    "sin",
    "segun",
    "como",
    "que",
    "incluso",
    "incluido",
    "incluida",
    "incluye",
    "mediante",
    "totalmente",
    "instalado",
    "instalada",
    "instalados",
    "instaladas",
    "instalacion",
    "montaje",
    "montado",
    "montada",
    "suministro",
    "colocado",
    "colocada",
    "colocacion",
    "medida",
    "medido",
    "unidad",
    "partida",
    "tipo",
    "marca",
    "modelo",
    "similar",
    "equivalente",
    "otro",
    "otra",
    "todo",
    "toda",
    "cada",
    "mas",
    "pequeno",
    "material",
    "medios",
    "auxiliares",
    "necesario",
    "necesarios",
    "funcionando",
    "probado",
    "probada",
    "conexion",
    "conexionado",
    "ejecutado",
    "ejecutada",
    "and",
    "the",
];

fn sin_tildes(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        '×' => 'x',
        '⌀' => 'ø',
        c => c,
    }
}

/// Unidad normalizada para comparar.
pub fn unidad(u: &str) -> String {
    let u: String = u
        .trim()
        .to_lowercase()
        .chars()
        .map(sin_tildes)
        .filter(|c| !c.is_whitespace() && *c != '.')
        .collect();
    match u.as_str() {
        "ml" | "m" | "mts" | "metro" | "metros" => "m".into(),
        "m2" | "m²" => "m2".into(),
        "m3" | "m³" => "m3".into(),
        "ud" | "u" | "un" | "uds" | "ut" | "unid" | "unidad" => "ud".into(),
        "pa" | "p.a" | "pa." => "pa".into(),
        _ => u,
    }
}

/// Datos técnicos y palabras de un texto.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rasgos {
    pub tecnicos: BTreeSet<String>,
    pub palabras: BTreeSet<String>,
    /// Palabras que solo están en el texto largo (cuentan menos).
    pub secundarias: BTreeSet<String>,
}

fn es_num(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.') && s.chars().any(|c| c.is_ascii_digit())
}

/// «4.0» → «4», «0.80» → «0.8».
fn numero(n: &str) -> String {
    if n.contains('.') {
        n.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        n.to_owned()
    }
}

fn singular(p: &str) -> String {
    if p.len() > 5 && p.ends_with("es") && !p.ends_with("tes") {
        p[..p.len() - 2].to_owned()
    } else if p.len() > 4 && p.ends_with('s') {
        p[..p.len() - 1].to_owned()
    } else {
        p.to_owned()
    }
}

/// Serie y tamaño de un modelo con guiones o barras: letras iniciales y hasta
/// tres números enteros antes del primer decimal («chgt/4-560-6/-0.55» →
/// «chgt:4-560-6», «cvht-h-15/15-1.1kw» → «cvht:15-15»).
fn clave_modelo(w: &str) -> Option<String> {
    if !w.contains(['-', '/']) {
        return None;
    }
    let serie: String = w.chars().take_while(char::is_ascii_alphabetic).collect();
    if serie.len() < 2 {
        return None;
    }
    let mut nums = Vec::new();
    for trozo in w[serie.len()..].split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let trozo = trozo.trim_matches('.');
        if trozo.is_empty() {
            continue;
        }
        if trozo.contains('.') || nums.len() == 3 {
            break;
        }
        nums.push(trozo.to_owned());
    }
    (!nums.is_empty()).then(|| format!("{serie}:{}", nums.join("-")))
}

/// Potencia escrita dentro de un modelo («…-1.1kw-…» → «1.1kw»).
fn potencia_dentro(w: &str) -> Option<String> {
    let i = w.find("kw")?;
    let n: String = w[..i]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let n = numero(n.trim_start_matches('.'));
    (!n.is_empty() && i < w.len())
        .then(|| format!("{n}kw"))
        .filter(|_| w.len() > i + 2 || w.contains(['-', '/']))
}

/// Dos datos técnicos coinciden si son iguales o uno es el otro con un
/// sufijo tras guion («20-45-h-o» y «20-45-h»).
fn coincide(a: &str, b: &str) -> bool {
    let sufijo =
        |x: &str, y: &str| x.len() > y.len() && y.len() >= 4 && x.starts_with(y) && x[y.len()..].starts_with('-');
    a == b || sufijo(a, b) || sufijo(b, a)
}

/// Etiquetas de la medición («E1», «A6», «P3»): no son datos técnicos.
fn es_etiqueta(w: &str) -> bool {
    let mut c = w.chars();
    matches!(c.next(), Some(l) if l.is_ascii_alphabetic() && l != 'd') && w.len() <= 3 && c.all(|x| x.is_ascii_digit())
}

/// Medida «400x300» → (400, 300).
fn medida(t: &str) -> Option<(u64, u64)> {
    let (a, b) = t.split_once('x')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Saca los rasgos de un texto.
pub fn rasgos(texto: &str) -> Rasgos {
    // Normalizar: minúsculas, sin tildes, coma decimal → punto, separadores → espacio
    let bajo: Vec<char> = texto.to_lowercase().chars().map(sin_tildes).collect();
    let mut t = String::with_capacity(bajo.len());
    for (i, &c) in bajo.iter().enumerate() {
        let entre_cifras = |k: usize| {
            i > 0
                && bajo.get(i - 1).is_some_and(char::is_ascii_digit)
                && bajo.get(i + k).is_some_and(char::is_ascii_digit)
        };
        match c {
            ',' if entre_cifras(1) => t.push('.'),
            'ø' => t.push_str(" d"),
            '=' | ':' => {}
            c if c.is_alphanumeric() || matches!(c, '.' | '/' | '-' | '+') => t.push(c),
            _ => t.push(' '),
        }
    }
    let crudos: Vec<String> = t
        .split_whitespace()
        .map(|s| s.trim_matches(|c| matches!(c, '.' | '-' | '/' | '+')).to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let mut r = Rasgos::default();
    let mut i = 0;
    while i < crudos.len() {
        let w = &crudos[i];
        let sig = crudos.get(i + 1).map(String::as_str);
        // «d 125», «dn 50», «ø 125»
        if matches!(w.as_str(), "d" | "dn" | "o") && sig.is_some_and(es_num) {
            let n = sig.unwrap_or_default().trim_end_matches("mm");
            r.tecnicos.insert(format!("{}{n}", if w == "dn" { "dn" } else { "d" }));
            i += 2;
            continue;
        }
        // «0.55 kw», «125 mm», «400 x 300»
        if es_num(w) {
            match sig {
                Some(u @ ("kw" | "w" | "mm" | "m3/h" | "l/s" | "pa" | "bar" | "kg" | "v")) => {
                    let w = numero(w);
                    r.tecnicos.insert(if u == "mm" { w } else { format!("{w}{u}") });
                    i += 2;
                    continue;
                }
                Some("x") if crudos.get(i + 2).is_some_and(|s| es_num(s)) => {
                    r.tecnicos.insert(format!("{w}x{}", crudos[i + 2]));
                    i += 3;
                    continue;
                }
                // «2 zonas», «4 extractores»: número y lo que cuenta
                Some(p)
                    if w.len() <= 2
                        && !w.contains('.')
                        && p.chars().count() >= 4
                        && p.chars().all(char::is_alphabetic)
                        && !VACIAS.contains(&p) =>
                {
                    r.tecnicos.insert(format!("{w}{}", singular(p)));
                    i += 1;
                    continue;
                }
                _ => {}
            }
        }
        let letras = w.chars().any(char::is_alphabetic);
        let cifras = w.chars().any(|c| c.is_ascii_digit());
        if cifras {
            for w in w.split('+').filter(|x| !x.is_empty()) {
                let w = w.trim_end_matches("mm");
                if es_etiqueta(w) || w.contains("hz") {
                    continue;
                }
                // «4.0kw» → «4kw»
                let w = match w.find(|c: char| c.is_alphabetic()) {
                    Some(i) if i > 0 && es_num(&w[..i]) => format!("{}{}", numero(&w[..i]), &w[i..]),
                    _ => w.to_owned(),
                };
                let w = w.as_str();
                if letras || w.contains(['/', '-', 'x']) || w.trim_matches('.').len() >= 2 {
                    // «d125mm» → «d125»; «dn50»; «chgt/4-560-6/-0.55»
                    let w = w
                        .strip_prefix("o")
                        .filter(|x| es_num(x))
                        .map_or(w.to_owned(), |x| format!("d{x}"));
                    // Modelo: serie y tamaño, sin potencia ni sufijos
                    // («CHGT/4-560-6/-0,55» y «CHGT/4-560-6/14A» → «chgt:4-560-6»;
                    // «CVHT-H-15/15-1,1kW-700RPM» y «CVHT-15/15-1,1» → «cvht:15-15»);
                    // la potencia que lleve dentro va aparte («1.1kw»)
                    if let Some(k) = potencia_dentro(&w) {
                        r.tecnicos.insert(k);
                    }
                    match clave_modelo(&w) {
                        Some(k) => {
                            r.tecnicos.insert(k);
                        }
                        None => {
                            r.tecnicos.insert(w);
                        }
                    }
                }
            }
        } else if letras && w.chars().count() >= 3 && !VACIAS.contains(&w.as_str()) {
            r.palabras.insert(singular(w));
        }
        i += 1;
    }
    r
}

/// Puntuación (0–100) de `base` para `medicion`: parte de los rasgos de la
/// medición que están en la base, con los técnicos al triple. Una medida que
/// no está cuenta en parte si la base tiene la inmediata superior (la más
/// cercana, más). `extra` son datos técnicos del texto de la medición: un
/// modelo que esté en la base suma.
pub fn puntuar(medicion: &Rasgos, extra: &BTreeSet<String>, base: &Rasgos) -> u32 {
    const T: u64 = 300;
    const P: u64 = 100;
    let total = T * medicion.tecnicos.len() as u64 + P * medicion.palabras.len() as u64;
    if total == 0 {
        return 0;
    }
    let mut exactos = 0;
    let mut suma = 0u64;
    for t in &medicion.tecnicos {
        if base.tecnicos.contains(t) || base.tecnicos.iter().any(|b| coincide(t, b)) {
            exactos += 1;
            suma += T;
        } else if let Some((a, b)) = medida(t) {
            // Medida superior más cercana (hasta el doble de superficie)
            let mejor = base
                .tecnicos
                .iter()
                .filter_map(|x| medida(x))
                .filter(|&(c, d)| c >= a && d >= b && c * d <= 2 * a * b)
                .map(|(c, d)| a * b * 1000 / (c * d))
                .max();
            if let Some(r) = mejor {
                suma += T * 8 * r / 10_000;
            }
        }
    }
    // Abreviaturas («helic.» → «helicoidal»)
    let empieza = |set: &BTreeSet<String>, w: &str| w.len() >= 4 && set.iter().any(|b| b.starts_with(w));
    for w in &medicion.palabras {
        if base.palabras.contains(w) || empieza(&base.palabras, w) {
            suma += P;
        } else if base.secundarias.contains(w) || empieza(&base.secundarias, w) {
            suma += P * 6 / 10;
        }
    }
    // Un modelo del texto de la medición («Modelo FABA/4-560-6») que está en la base
    let es_modelo = |t: &str| t.chars().any(char::is_alphabetic) && t.contains(['/', '-']);
    let bonus = if extra
        .iter()
        .any(|t| es_modelo(t) && !medicion.tecnicos.contains(t) && base.tecnicos.contains(t))
    {
        T
    } else {
        0
    };
    let mut p = ((suma + bonus) * 100 / total).min(100) as u32;
    if !medicion.tecnicos.is_empty() && exactos == 0 && bonus == 0 {
        p = p.min(SEGURO - 1);
    }
    p
}

/// Clave con la que se recuerda una pareja confirmada: resumen normalizado y
/// unidad.
pub fn clave(resumen: &str, ud: &str) -> String {
    let r = rasgos(resumen);
    let mut v: Vec<String> = r.tecnicos.into_iter().chain(r.palabras).collect();
    v.sort();
    format!("{}|{}", v.join(" "), unidad(ud))
}

struct Entrada {
    codigo: String,
    unidad: String,
    rasgos: Rasgos,
}

/// Partidas de una base preparadas para buscar.
pub struct Indice {
    entradas: Vec<Entrada>,
}

impl Indice {
    pub fn de(base: &Presupuesto) -> Self {
        let entradas = base
            .conceptos
            .values()
            .filter(|c| c.naturaleza == Naturaleza::Partida)
            .map(|c| {
                // Del texto solo las palabras: sus números (normas, rpm,
                // rangos de medidas) confundirían
                let mut r = rasgos(&c.resumen);
                if let Some(t) = &c.texto {
                    r.secundarias = rasgos(t).palabras.difference(&r.palabras).cloned().collect();
                }
                Entrada {
                    codigo: c.codigo.clone(),
                    unidad: unidad(&c.unidad),
                    rasgos: r,
                }
            })
            .collect();
        Self { entradas }
    }

    pub fn len(&self) -> usize {
        self.entradas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entradas.is_empty()
    }

    /// Las `n` mejores partidas de `base` para una partida de la medición
    /// (resumen, texto y unidad). El texto solo aporta datos técnicos.
    pub fn candidatos(&self, base: &Presupuesto, resumen: &str, texto: &str, ud: &str, n: usize) -> Vec<Candidato> {
        let m = rasgos(resumen);
        let extra = rasgos(texto).tecnicos;
        let u = unidad(ud);
        let mut v: Vec<(u32, &Entrada)> = self
            .entradas
            .iter()
            .filter(|e| u.is_empty() || e.unidad.is_empty() || e.unidad == u)
            .map(|e| (puntuar(&m, &extra, &e.rasgos), e))
            .filter(|(p, _)| *p > 0)
            .collect();
        // A igualdad, la partida más concreta (menos rasgos) y el código más corto
        // (la variante «.S» de sala de máquinas detrás).
        let tam = |e: &Entrada| e.rasgos.tecnicos.len() + e.rasgos.palabras.len();
        v.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| tam(a.1).cmp(&tam(b.1)))
                .then_with(|| a.1.codigo.len().cmp(&b.1.codigo.len()))
                .then_with(|| a.1.codigo.cmp(&b.1.codigo))
        });
        // Empate con otra partida que no es su variante «.S»
        let raiz = |c: &str| c.strip_suffix(".S").unwrap_or(c).to_owned();
        let empate = v.len() > 1
            && v[1..]
                .iter()
                .take_while(|(p, _)| *p == v[0].0)
                .any(|(_, e)| raiz(&e.codigo) != raiz(&v[0].1.codigo));
        v.into_iter()
            .take(n)
            .filter_map(|(p, e)| {
                let c = base.conceptos.get(&e.codigo)?;
                Some(Candidato {
                    codigo: c.codigo.clone(),
                    unidad: c.unidad.clone(),
                    resumen: c.resumen.clone(),
                    precio: base.precio(&c.codigo).unwrap_or_default(),
                    puntos: p,
                    recordado: false,
                    empate,
                })
            })
            .collect()
    }
}

/// Partidas de la obra sin precio (sin descompuesto y a 0), en el orden del
/// árbol.
pub fn sin_precio(p: &Presupuesto) -> Vec<String> {
    let mut v = Vec::new();
    let mut vistos = BTreeSet::new();
    let mut pila = vec![p.raiz.clone()];
    while let Some(c) = pila.pop() {
        let Some(x) = p.conceptos.get(&c) else { continue };
        if !vistos.insert(c.clone()) {
            continue;
        }
        if x.naturaleza == Naturaleza::Partida {
            if x.descomposicion.is_empty() && x.precio.is_zero() {
                v.push(c);
            }
            continue;
        }
        pila.extend(x.descomposicion.iter().rev().map(|l| l.hijo.clone()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concepto::Concepto;
    use rust_decimal_macros::dec;

    #[test]
    fn rasgos_tecnicos() {
        let r = rasgos("Rejilla Acme 20-H 400 x 300 con Ø125, DN 50, 0,55 kW y d=200mm de aluminio");
        for t in ["400x300", "d125", "dn50", "0.55kw", "d200", "20-h"] {
            assert!(r.tecnicos.contains(t), "{t} en {:?}", r.tecnicos);
        }
        assert!(r.palabras.contains("rejilla") && r.palabras.contains("aluminio"));
        assert!(!r.palabras.contains("con"));
        let r = rasgos("Ventilador FABA/4-560-6/-0,55 400 ºC/2h");
        assert!(r.tecnicos.contains("faba:4-560-6"), "{:?}", r.tecnicos);
        let a = rasgos("CAJA DE VENTILACION CVHT-H-15/15-1,1kW-700RPM-F400");
        let b = rasgos("Caja de ventilación F400 S&P CVHT-15/15-1,1 (1,1 kW, 9.000 m³/h)");
        assert!(
            a.tecnicos.contains("cvht:15-15") && b.tecnicos.contains("cvht:15-15"),
            "{a:?} {b:?}"
        );
        assert!(a.tecnicos.contains("1.1kw") && b.tecnicos.contains("1.1kw"));
        assert!(coincide("20-45-h-o", "20-45-h") && !coincide("20-45-h", "20-45-v"));
        assert!(rasgos("Central de detección de CO 2 zonas").tecnicos.contains("2zona"));
        assert_eq!(
            rasgos("FABA/4-560-6/14A").tecnicos,
            rasgos("faba/4-560-6/-0,55").tecnicos
        );
        assert_eq!(rasgos("Tuberías de cobre").palabras, rasgos("tubería cobre").palabras);
        assert_eq!(rasgos("4,0 kW").tecnicos, rasgos("4kw").tecnicos);
        assert!(rasgos("Cuadro E1 y A2+A3, 230/400V50HZ").tecnicos.is_empty());
    }

    #[test]
    fn unidades_equivalentes() {
        assert_eq!(unidad("ML"), "m");
        assert_eq!(unidad("m²"), "m2");
        assert_eq!(unidad("Ud."), "ud");
    }

    fn base() -> Presupuesto {
        let mut b = Presupuesto::new("t", "BASE", "Base");
        for (c, u, r, pr) in [
            (
                "REJ.400",
                "ud",
                "Rejilla de retorno Acme 20-H 400x300 instalada",
                dec!(40),
            ),
            (
                "REJ.500",
                "ud",
                "Rejilla de retorno Acme 20-H 500x300 instalada",
                dec!(48),
            ),
            ("TUB.25", "m", "Tubería de cobre Ø25 aislada", dec!(18)),
            ("VEN.1", "ud", "Ventilador helicoidal FABA/4-560-6 0,55 kW", dec!(900)),
        ] {
            let mut x = Concepto::basico(c, u, r, Naturaleza::Partida, pr);
            x.texto = Some("Suministro y montaje.".into());
            b.conceptos.insert(c.into(), x);
        }
        b
    }

    #[test]
    fn emparejar_por_datos_tecnicos_y_unidad() {
        let b = base();
        let i = Indice::de(&b);
        assert_eq!(i.len(), 4);
        let c = i.candidatos(&b, "Rejilla retorno 400 x 300 mm", "", "Ud", 3);
        assert_eq!(c[0].codigo, "REJ.400");
        assert_eq!(c[0].nivel(), Nivel::Seguro);
        assert!(c[1].puntos < c[0].puntos);
        // La unidad manda: en metros no hay rejillas
        let c = i.candidatos(&b, "Tubería cobre d=25 mm", "", "ml", 3);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].codigo.as_str(), c[0].nivel()), ("TUB.25", Nivel::Seguro));
        // Medida que no está: como mucho dudosa
        let c = i.candidatos(&b, "Rejilla de retorno 600x600", "", "ud", 3);
        assert!(c.iter().all(|c| c.nivel() != Nivel::Seguro));
        // Sin nada en común
        assert!(i.candidatos(&b, "Extintor polvo", "", "ud", 3).is_empty());
        // El modelo en el texto también cuenta
        let c = i.candidatos(&b, "Ventilador de extracción", "Modelo FABA/4-560-6, 0,55 kW", "ud", 1);
        assert_eq!((c[0].codigo.as_str(), c[0].precio), ("VEN.1", dec!(900)));
    }

    #[test]
    fn clave_estable_y_partidas_sin_precio() {
        assert_eq!(
            clave("Rejilla  400x300 de retorno", "UD"),
            clave("rejilla retorno 400 x 300", "ud")
        );
        let mut p = crate::ejemplos::suelo_radiante();
        let n = sin_precio(&p).len();
        let cap = p.conceptos[&p.raiz].descomposicion[0].hijo.clone();
        p.nueva_partida(&cap, "X.1", "ud", "Partida sin precio").unwrap();
        assert_eq!(sin_precio(&p).len(), n + 1);
        assert!(sin_precio(&p).contains(&"X.1".to_owned()));
    }

    #[test]
    fn componer_una_partida_con_varias_de_la_base() {
        let mut b = Presupuesto::new("t", "BASE", "Base");
        b.nuevo_capitulo("BASE", "C", "C").unwrap();
        for (c, r) in [
            ("VEN", "Ventilador 1,1 kW"),
            ("VAR", "Variador 1,1 kW"),
            ("COM", "Compuerta"),
        ] {
            b.nueva_partida("C", c, "ud", r).unwrap();
        }
        b.nuevo_recurso("MT.V", "ud", "Ventilador", Naturaleza::Material, Decimal::ZERO)
            .unwrap();
        b.fijar_tarifa_grupo("MT.V", "Faba", Some("E04"), dec!(1000)).unwrap();
        b.fijar_descuento_texto("Faba/E04", "60+10").unwrap();
        b.anadir_linea("VEN", "MT.V", Decimal::ONE).unwrap();
        b.nuevo_recurso("MT.X", "ud", "Variador", Naturaleza::Material, dec!(200))
            .unwrap();
        b.anadir_linea("VAR", "MT.X", Decimal::ONE).unwrap();
        b.nuevo_recurso("MT.C", "ud", "Compuerta", Naturaleza::Material, dec!(50))
            .unwrap();
        b.anadir_linea("COM", "MT.C", Decimal::ONE).unwrap();
        let mut o = Presupuesto::new("t", "O", "O");
        o.nuevo_capitulo("O", "K", "K").unwrap();
        o.nueva_partida("K", "01", "ud", "VENTILADOR 1,1KW + VARIADOR + 2 COMPUERTAS")
            .unwrap();
        o.nueva_partida("K", "02", "ud", "Ventilador").unwrap();
        let partes = vec![
            ("VEN".to_owned(), dec!(1)),
            ("VAR".into(), dec!(1)),
            ("COM".into(), dec!(2)),
        ];
        o.componer_de(&b, "01", &partes).unwrap();
        // 1000 − 60+10 % = 360; + 200 + 2 × 50
        assert_eq!(o.precio("01").unwrap(), dec!(660));
        assert_eq!(o.texto_descuento("Faba/E04"), "60+10");
        assert_eq!(o.conceptos["01"].descomposicion.len(), 3);
        assert_eq!(o.conceptos["01"].resumen, "VENTILADOR 1,1KW + VARIADOR + 2 COMPUERTAS");
        // Ya compuesta: no se sustituye; con una sola parte trae el descompuesto plano
        assert!(o.componer_de(&b, "01", &partes).is_err());
        o.componer_de(&b, "02", &[("VEN".to_owned(), Decimal::ONE)]).unwrap();
        assert_eq!(o.conceptos["02"].descomposicion[0].hijo, "MT.V");
        // Un fallo deja la obra como estaba
        o.nueva_partida("K", "03", "ud", "x").unwrap();
        let antes = o.clone();
        assert!(
            o.componer_de(&b, "03", &[("VEN".to_owned(), dec!(1)), ("NO".into(), dec!(1))])
                .is_err()
        );
        assert_eq!(o, antes);
        assert!(o.componer_de(&b, "03", &[]).is_err());
    }
}
