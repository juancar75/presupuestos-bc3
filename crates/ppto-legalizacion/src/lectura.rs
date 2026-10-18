// SPDX-License-Identifier: GPL-3.0-or-later
//! Lectura del presupuesto para proponer los datos del expediente de
//! legalización: potencias RITE, unidades exteriores (circuitos RSIF),
//! refrigerante, instalación frigorífica, sistemas PCI, BIE y detección.
//!
//! Solo **propone**: cada dato va con la partida de la que sale para que el
//! técnico lo revise. Lo que no está en las mediciones (uso del DB SI 4,
//! superficie, viviendas, revisiones, visitas) no se toca.

use crate::{CAPITULO, Disciplina, Expediente, Refrigerante, RegistroIf};
use ppto_core::formato::num;
use ppto_core::{Naturaleza, Presupuesto};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet};

/// Un dato propuesto y de dónde sale.
#[derive(Debug, Clone, PartialEq)]
pub struct Pista {
    pub dato: String,
    pub fuente: String,
    /// `false` si depende de cómo esté redactada la partida y conviene revisarlo.
    pub fiable: bool,
}

/// Datos sacados del presupuesto.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lectura {
    /// Capítulos de primer nivel que parecen de cada disciplina (base del PEM).
    pub capitulos_rite: Vec<String>,
    pub capitulos_pci: Vec<String>,
    // RITE
    pub potencia_calor: Decimal,
    pub potencia_frio: Decimal,
    /// Unidades exteriores y enfriadoras (circuitos frigoríficos) y modelos distintos.
    pub circuitos: u32,
    pub modelos: u32,
    pub primario_frio: bool,
    pub sala_maquinas: bool,
    pub refrigerante: Option<Refrigerante>,
    // Instalación frigorífica (cámaras, murales, centrales)
    pub frigorifica: bool,
    pub if_kw_sistema: Decimal,
    pub if_kw_total: Decimal,
    pub atmosfera_artificial: bool,
    // PCI
    pub sistemas_pci: Vec<&'static str>,
    pub bie: u32,
    pub detectores: u32,
    pub deteccion: bool,
    pub presion_diferencial: bool,
    /// Generadores sin potencia legible en el texto.
    pub sin_potencia: Vec<String>,
    pub pistas: Vec<Pista>,
}

impl Lectura {
    pub fn vacia(&self) -> bool {
        self.pistas.is_empty()
    }

    /// Registro frigorífico que corresponde a lo leído (memoria A2L o L1 en
    /// nivel 1; proyecto en nivel 2), si hay instalación frigorífica o
    /// refrigerante A2L.
    pub fn registro_if(&self) -> Option<RegistroIf> {
        if !self.frigorifica && self.refrigerante != Some(Refrigerante::A2l) {
            return None;
        }
        let nivel2 = self.if_kw_sistema > Decimal::from(30)
            || self.if_kw_total > Decimal::from(100)
            || self.atmosfera_artificial
            || matches!(self.refrigerante, Some(Refrigerante::L2 | Refrigerante::L3));
        Some(if nivel2 {
            RegistroIf::Proyecto
        } else if self.refrigerante == Some(Refrigerante::A2l) {
            RegistroIf::MemoriaA2l
        } else {
            RegistroIf::MemoriaL1
        })
    }

    /// Copia en el expediente los datos leídos (los no leídos se dejan).
    pub fn rellenar(&self, e: &mut Expediente) {
        if !self.potencia_calor.is_zero() {
            e.potencia_calor_kw = self.potencia_calor;
        }
        if !self.potencia_frio.is_zero() {
            e.potencia_frio_kw = self.potencia_frio;
        }
        if e.disciplina == Disciplina::Rite && self.circuitos > 0 {
            e.circuitos_rsif = self.circuitos;
            e.modelos_rsif = self.modelos;
        }
        e.primario_frio |= self.primario_frio;
        e.sala_maquinas |= self.sala_maquinas;
        if let Some(r) = self.refrigerante {
            e.refrigerante = r;
        }
        if let Some(r) = self.registro_if() {
            e.registro_if = Some(r);
        }
        if !self.if_kw_sistema.is_zero() {
            e.if_kw_sistema = self.if_kw_sistema;
            e.if_kw_total = self.if_kw_total;
        }
        e.atmosfera_artificial |= self.atmosfera_artificial;
        if !self.sistemas_pci.is_empty() {
            e.instalaciones_pci = self.sistemas_pci.len() as u32;
        }
        e.pci_bie |= self.bie > 0;
        e.pci_deteccion |= self.deteccion;
        e.presion_diferencial |= self.presion_diferencial;
    }
}

/// Minúsculas y sin tildes, para buscar palabras clave.
fn normalizar(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            '\n' | '\r' | '\t' => ' ',
            c => c,
        })
        .collect()
}

fn alguna(t: &str, claves: &[&str]) -> bool {
    claves.iter().any(|k| t.contains(k))
}

/// Partidas que no son equipos aunque los nombren («Tubería para unidad
/// exterior», «Señal de BIE»…).
const NO_EQUIPO: &[&str] = &[
    "tuberia",
    "linea",
    "soporte",
    "bancada",
    "desague",
    "conexion",
    "cable",
    "canalizacion",
    "aislamiento",
    "rejilla",
    "difusor",
    "conducto",
    "prueba",
    "puesta en marcha",
    "legalizacion",
    "mando",
    "control",
    "termostato",
    "unidad interior",
    "ud interior",
    "ud. interior",
    "u. interior",
    "fancoil",
    "fan-coil",
    "fan coil",
    "cassette",
    "casete",
    "climatizador",
    "recuperador",
    "bomba circuladora",
    "bomba de circulacion",
    "deposito",
    "vaso de expansion",
    "valvula",
    "kit",
    "senal",
    "cartel",
    "placa",
    "manguera",
    "carga de",
    "recarga",
    "revision",
    "mantenimiento",
    "desmontaje",
    "retirada",
    "derivacion",
    "estructura",
    "ocultacion",
    "cerramiento",
    "celosia",
    "bandeja",
    "amortiguador",
    "antivibratorio",
    "ayudas",
    "documentacion",
    "certificado",
    "proyecto",
];

fn no_es_equipo(resumen_norm: &str) -> bool {
    let r = resumen_norm.trim_start_matches(|c: char| !c.is_alphanumeric());
    // Sin la unidad del principio («Ud.», «Ml.», «m2», «u»).
    let r = match r.split_once(' ') {
        Some((u, resto)) if u.len() <= 4 && (u.ends_with('.') || matches!(u, "ud" | "u" | "ml" | "m" | "m2")) => {
            resto.trim_start()
        }
        _ => r,
    };
    NO_EQUIPO.iter().any(|k| r.starts_with(k))
}

/// Generadores RITE (calor, frío o ambos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Generador {
    Calor,
    Frio,
    Ambos,
}

fn generador(t: &str) -> Option<Generador> {
    if alguna(t, &["caldera"]) {
        return Some(Generador::Calor);
    }
    if alguna(t, &["enfriadora"]) && !t.contains("bomba de calor") {
        return Some(Generador::Frio);
    }
    if alguna(
        t,
        &[
            "unidad exterior",
            "ud exterior",
            "ud. exterior",
            "u. exterior",
            "bomba de calor",
            "rooftop",
            "roof-top",
            "roof top",
            "equipo autonomo",
            "equipo compacto",
            "split",
            "vrf",
            "vrv",
            "aerotermia",
            "enfriadora",
        ],
    ) {
        return Some(if t.contains("solo frio") {
            Generador::Frio
        } else {
            Generador::Ambos
        });
    }
    None
}

const FRIGORIFICA: &[&str] = &[
    "central frigorifica",
    "central de compresores",
    "grupo de compresores",
    "unidad condensadora",
    "condensador remoto",
    "camara frigorifica",
    "camara de conservacion",
    "camara de congelacion",
    "camara de refrigeracion",
    "mueble frigorifico",
    "mural frigorifico",
    "murales frigorificos",
    "vitrina frigorifica",
    "isla de congelados",
    "equipo frigorifico",
];
const FRIGORIFICA_EQUIPO: &[&str] = &[
    "central frigorifica",
    "central de compresores",
    "grupo de compresores",
    "unidad condensadora",
    "equipo frigorifico",
    "compresor",
];

/// Sistemas del RIPCI (anexo I) y sus palabras clave.
const SISTEMAS_PCI: &[(&str, &[&str])] = &[
    ("extintores", &["extintor"]),
    ("BIE", &["boca de incendio"]),
    (
        "detección",
        &[
            "detector",
            "central de deteccion",
            "deteccion de incendio",
            "deteccion automatica",
        ],
    ),
    ("alarma", &["pulsador", "sirena", "alarma de incendio", "avisador"]),
    ("hidrantes", &["hidrante"]),
    ("columna seca", &["columna seca"]),
    ("rociadores", &["rociador", "sprinkler"]),
    (
        "abastecimiento de agua",
        &[
            "grupo de presion contra incendios",
            "grupo contra incendios",
            "grupo de bombeo contra incendios",
            "grupo de presion pci",
            "abastecimiento de agua contra incendios",
            "aljibe contra incendios",
            "deposito contra incendios",
        ],
    ),
    (
        "extinción automática",
        &[
            "extincion automatica",
            "agente extintor",
            "fm-200",
            "fm200",
            "novec",
            "agua nebulizada",
        ],
    ),
    (
        "control de humos",
        &[
            "control de humos",
            "extraccion de humos",
            "sobrepresion",
            "presion diferencial",
            "exutorio",
        ],
    ),
    (
        "señalización",
        &["luminiscente", "senalizacion de evacuacion", "senal de evacuacion"],
    ),
];

/// «BIE» como palabra en el texto original (no «BIEN»).
fn menciona_bie(original: &str) -> bool {
    original
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == "BIE" || w == "BIEs" || w == "B.I.E")
}

/// Grupo del refrigerante más exigente nombrado en el texto.
fn refrigerante(original: &str) -> Option<(Refrigerante, String)> {
    let t = original.to_uppercase();
    let b = t.as_bytes();
    let mut mejor: Option<(Refrigerante, String)> = None;
    let rango = |r: Refrigerante| match r {
        Refrigerante::L1 => 0,
        Refrigerante::A2l => 1,
        Refrigerante::L2 => 2,
        Refrigerante::L3 => 3,
    };
    for (i, _) in t.match_indices('R') {
        if i > 0 && (b[i - 1] as char).is_alphanumeric() {
            continue;
        }
        let resto = &t[i + 1..];
        let resto = resto.strip_prefix(['-', ' ']).unwrap_or(resto);
        let codigo: String = resto.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if !codigo.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let grupo = match codigo.as_str() {
            "32" | "454B" | "454C" | "1234YF" | "1234ZE" | "452B" => Refrigerante::A2l,
            "410A" | "407C" | "134A" | "513A" | "448A" | "449A" | "404A" | "507" | "507A" | "744" | "450A" | "422D"
            | "427A" | "22" => Refrigerante::L1,
            "717" => Refrigerante::L2,
            "290" | "600A" | "1270" => Refrigerante::L3,
            _ => continue,
        };
        if mejor.as_ref().is_none_or(|(m, _)| rango(grupo) > rango(*m)) {
            mejor = Some((grupo, format!("R-{codigo}")));
        }
    }
    mejor
}

/// Qué indica la palabra junto a una potencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rotulo {
    Frio,
    Calor,
    Electrica,
    Otro,
}

const ROTULOS: &[(&str, Rotulo)] = &[
    ("frigorifica", Rotulo::Frio),
    ("frio", Rotulo::Frio),
    ("refrigeracion", Rotulo::Frio),
    ("enfriamiento", Rotulo::Frio),
    ("pf", Rotulo::Frio),
    ("cooling", Rotulo::Frio),
    ("calorifica", Rotulo::Calor),
    ("calor", Rotulo::Calor),
    ("calefaccion", Rotulo::Calor),
    ("pc", Rotulo::Calor),
    ("heating", Rotulo::Calor),
    ("absorbida", Rotulo::Electrica),
    ("electrica", Rotulo::Electrica),
    ("consumo", Rotulo::Electrica),
    ("minima", Rotulo::Otro),
    ("maxima", Rotulo::Otro),
];

/// Palabra-rótulo más cercana en `ventana` (texto normalizado): la última si
/// `antes`, la primera si no.
fn rotulo(ventana: &str, antes: bool) -> Option<Rotulo> {
    let mut mejor: Option<(usize, Rotulo)> = None;
    for (k, r) in ROTULOS {
        let pos = if antes { ventana.rfind(k) } else { ventana.find(k) };
        if let Some(p) = pos {
            // Palabra completa para las siglas cortas.
            if k.len() <= 2 {
                let bs = ventana.as_bytes();
                let antes_ok = p == 0 || !(bs[p - 1] as char).is_alphanumeric();
                let despues_ok = p + k.len() >= bs.len() || !(bs[p + k.len()] as char).is_alphanumeric();
                if !(antes_ok && despues_ok) {
                    continue;
                }
            }
            let mejor_pos = if antes { p + k.len() } else { usize::MAX - p };
            if mejor.is_none_or(|(m, _)| mejor_pos > m) {
                mejor = Some((mejor_pos, *r));
            }
        }
    }
    mejor.map(|m| m.1)
}

fn leer_numero(s: &str) -> Option<Decimal> {
    let s = if s.contains(',') {
        s.replace('.', "").replace(',', ".")
    } else if let Some((_, dec)) = s.split_once('.')
        && dec.len() == 3
    {
        s.replace('.', "")
    } else {
        s.to_owned()
    };
    s.parse().ok()
}

/// Potencias en kW del texto con su rótulo. Acepta «28 kW», «28,5kW»,
/// «28/31,5 kW» y frig/h o kcal/h (÷ 860).
fn potencias(t: &str) -> Vec<(Decimal, Option<Rotulo>)> {
    let chars: Vec<(usize, char)> = t.char_indices().collect();
    let mut out = Vec::new();
    let mut fin_anterior = 0;
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        let previo_letra = i > 0 && chars[i - 1].1.is_alphanumeric();
        if !c.is_ascii_digit() || previo_letra {
            i += 1;
            continue;
        }
        // Números seguidos de «/» (frío/calor) y unidad al final.
        let mut grupo: Vec<(usize, usize)> = Vec::new();
        let mut j = i;
        loop {
            let ini = j;
            while j < chars.len() && (chars[j].1.is_ascii_digit() || chars[j].1 == ',' || chars[j].1 == '.') {
                j += 1;
            }
            let mut fin = j;
            while fin > ini && !chars[fin - 1].1.is_ascii_digit() {
                fin -= 1;
            }
            grupo.push((ini, fin));
            j = fin;
            let mut k = j;
            while k < chars.len() && chars[k].1 == ' ' {
                k += 1;
            }
            if k < chars.len() && chars[k].1 == '/' {
                k += 1;
                while k < chars.len() && chars[k].1 == ' ' {
                    k += 1;
                }
                if k < chars.len() && chars[k].1.is_ascii_digit() {
                    j = k;
                    continue;
                }
            }
            break;
        }
        let mut k = j;
        while k < chars.len() && chars[k].1 == ' ' {
            k += 1;
        }
        let byte = |n: usize| chars.get(n).map_or(t.len(), |c| c.0);
        let resto = &t[byte(k)..];
        let (factor, largo) = if resto.starts_with("kw") && !resto.starts_with("kwh") {
            (Some(Decimal::ONE), 2)
        } else if resto.starts_with("frig/h") || resto.starts_with("kcal/h") {
            (Some(Decimal::ONE / Decimal::from(860)), 6)
        } else {
            (None, 0)
        };
        if let Some(f) = factor {
            let fin_unidad = byte(k) + largo;
            let despues: String = t[fin_unidad..]
                .chars()
                .take(14)
                .take_while(|c| !c.is_ascii_digit() && !matches!(c, ';' | ',' | '/'))
                .collect();
            let antes = &t[fin_anterior.min(pos)..pos];
            let antes: String = {
                let v: Vec<char> = antes.chars().collect();
                v[v.len().saturating_sub(40)..].iter().collect()
            };
            let r_antes = rotulo(&antes, true);
            let r_despues = rotulo(&despues, false);
            let n = grupo.len();
            // «28/31,5 kW» sin rótulo: frío y calor, en ese orden.
            let r = if n > 1 { None } else { r_despues.or(r_antes) };
            for (a, b) in grupo {
                if let Some(v) = leer_numero(&t[byte(a)..byte(b)]) {
                    out.push(((v * f).round_dp(2), r));
                }
            }
            fin_anterior = fin_unidad;
            i = k + largo;
        } else {
            i = j.max(i + 1);
        }
    }
    out
}

/// Potencia de frío y calor de un generador RITE (por unidad).
fn frio_calor(t: &str, g: Generador) -> Option<(Decimal, Decimal)> {
    let v: Vec<_> = potencias(t)
        .into_iter()
        .filter(|(_, r)| !matches!(r, Some(Rotulo::Electrica | Rotulo::Otro)))
        .collect();
    if v.is_empty() {
        return None;
    }
    let frio = v.iter().find(|(_, r)| *r == Some(Rotulo::Frio)).map(|x| x.0);
    let calor = v.iter().find(|(_, r)| *r == Some(Rotulo::Calor)).map(|x| x.0);
    let sin: Vec<Decimal> = v.iter().filter(|(_, r)| r.is_none()).map(|x| x.0).collect();
    let (frio, calor) = match g {
        Generador::Calor => (None, calor.or(sin.iter().copied().reduce(Decimal::max))),
        Generador::Frio => (frio.or(sin.first().copied()), None),
        Generador::Ambos => match (frio, calor, sin.as_slice()) {
            (Some(f), Some(c), _) => (Some(f), Some(c)),
            (Some(f), None, [c, ..]) => (Some(f), Some(*c)),
            (None, Some(c), [f, ..]) => (Some(*f), Some(c)),
            (Some(f), None, []) => (Some(f), None),
            (None, Some(c), []) => (None, Some(c)),
            (None, None, [f, c, ..]) => (Some(*f), Some(*c)),
            (None, None, [x]) => (Some(*x), Some(*x)),
            (None, None, []) => (None, None),
        },
    };
    if frio.is_none() && calor.is_none() {
        return None;
    }
    Some((frio.unwrap_or_default(), calor.unwrap_or_default()))
}

/// Potencia de compresores de un equipo frigorífico: la eléctrica si viene,
/// si no la primera que aparezca (y se marca para revisar).
fn kw_frigorifica(t: &str) -> Option<(Decimal, bool)> {
    let v = potencias(t);
    if let Some(e) = v.iter().find(|(_, r)| *r == Some(Rotulo::Electrica)) {
        return Some((e.0, true));
    }
    v.first().map(|x| (x.0, false))
}

/// Partida medida en la obra: cantidad total (producto de los rendimientos
/// de los capítulos por los que cuelga) y textos.
struct Medida {
    codigo: String,
    resumen: String,
    texto: String,
    cantidad: Decimal,
    unidad: String,
    /// Resúmenes de los recursos del descompuesto con su cantidad por unidad.
    recursos: Vec<(String, Decimal)>,
}

fn medidas(p: &Presupuesto) -> Vec<Medida> {
    let mut acumulado: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut pila = vec![(p.raiz.clone(), Decimal::ONE, 0usize)];
    while let Some((codigo, mult, prof)) = pila.pop() {
        let Some(c) = p.conceptos.get(&codigo) else { continue };
        if prof > 30 {
            continue;
        }
        for l in &c.descomposicion {
            if l.hijo == CAPITULO {
                continue;
            }
            let Some(h) = p.conceptos.get(&l.hijo) else { continue };
            let q = mult * l.cantidad();
            match h.naturaleza {
                Naturaleza::Capitulo => pila.push((h.codigo.clone(), q, prof + 1)),
                Naturaleza::Porcentaje => {}
                _ => *acumulado.entry(h.codigo.clone()).or_default() += q,
            }
        }
    }
    acumulado
        .into_iter()
        .filter_map(|(codigo, cantidad)| {
            let c = p.conceptos.get(&codigo)?;
            let recursos = c
                .descomposicion
                .iter()
                .filter_map(|l| {
                    let h = p.conceptos.get(&l.hijo)?;
                    matches!(
                        h.naturaleza,
                        Naturaleza::Material | Naturaleza::Otros | Naturaleza::Subcontrata
                    )
                    .then(|| (h.resumen.clone(), l.cantidad()))
                })
                .collect();
            Some(Medida {
                codigo,
                resumen: c.resumen.clone(),
                texto: c.texto.clone().unwrap_or_default(),
                cantidad,
                unidad: c.unidad.clone(),
                recursos,
            })
        })
        .collect()
}

fn cant(v: Decimal) -> String {
    let v = v.normalize();
    num(v, v.scale())
}

fn fuente(m: &Medida) -> String {
    let resumen: String = m.resumen.chars().take(70).collect();
    format!("{} {} × {} {}", m.codigo, resumen, cant(m.cantidad), m.unidad)
}

fn entero(v: Decimal) -> u32 {
    v.round().to_u32().unwrap_or(0)
}

/// Lee el presupuesto y propone los datos del expediente.
pub fn leer_presupuesto(p: &Presupuesto) -> Lectura {
    let mut l = Lectura::default();
    let pista = |l: &mut Lectura, dato: String, fuente: String, fiable: bool| {
        l.pistas.push(Pista { dato, fuente, fiable });
    };

    // Capítulos de primer nivel por disciplina.
    if let Some(raiz) = p.conceptos.get(&p.raiz) {
        for ln in &raiz.descomposicion {
            let Some(c) = p.conceptos.get(&ln.hijo) else { continue };
            if c.codigo == CAPITULO {
                continue;
            }
            let r = normalizar(&c.resumen);
            if alguna(&r, &["incendio", "pci", "contraincendio", "proteccion contra"]) {
                l.capitulos_pci.push(c.codigo.clone());
            } else if alguna(
                &r,
                &[
                    "climatizacion",
                    "calefaccion",
                    "hvac",
                    "aire acondicionado",
                    "clima",
                    "ventilacion",
                    "refrigeracion",
                    "frigorific",
                    "termica",
                ],
            ) {
                l.capitulos_rite.push(c.codigo.clone());
            }
        }
    }

    let mut modelos: BTreeSet<String> = BTreeSet::new();
    let mut sistemas: BTreeSet<&'static str> = BTreeSet::new();
    for m in medidas(p) {
        if m.cantidad <= Decimal::ZERO {
            continue;
        }
        let original = format!("{} {}", m.resumen, m.texto);
        let t = normalizar(&original);
        let r = normalizar(&m.resumen);
        let equipo = !no_es_equipo(&r);

        if let Some((g, nombre)) = refrigerante(&original) {
            let peor = match (l.refrigerante, g) {
                (None, _) => true,
                (Some(a), b) => rank(b) > rank(a),
            };
            if peor {
                l.refrigerante = Some(g);
                pista(
                    &mut l,
                    format!("Refrigerante {nombre} ({})", grupo(g)),
                    fuente(&m),
                    true,
                );
            }
        }
        if t.contains("sala de maquinas") && !l.sala_maquinas {
            l.sala_maquinas = true;
            pista(&mut l, "Sala de máquinas".into(), fuente(&m), false);
        }
        if t.contains("primario") && (t.contains("frigorific") || t.contains("enfriadora")) && !l.primario_frio {
            l.primario_frio = true;
            pista(&mut l, "Circuito primario frigorífico".into(), fuente(&m), false);
        }
        if alguna(
            &t,
            &["atmosfera controlada", "atmosfera artificial", "camara de maduracion"],
        ) && !l.atmosfera_artificial
        {
            l.atmosfera_artificial = true;
            pista(&mut l, "Cámaras de atmósfera artificial".into(), fuente(&m), false);
        }

        // Instalación frigorífica.
        let es_if = alguna(&t, FRIGORIFICA);
        if es_if {
            if !l.frigorifica {
                pista(&mut l, "Instalación frigorífica".into(), fuente(&m), true);
            }
            l.frigorifica = true;
            if equipo && alguna(&r, FRIGORIFICA_EQUIPO) {
                let leido = kw_frigorifica(&normalizar(&m.resumen)).or_else(|| kw_frigorifica(&normalizar(&m.texto)));
                if let Some((kw, electrica)) = leido {
                    l.if_kw_sistema = l.if_kw_sistema.max(kw);
                    l.if_kw_total += kw * m.cantidad;
                    pista(
                        &mut l,
                        format!(
                            "Frigorífica: {} kW por equipo{}",
                            cant(kw),
                            if electrica {
                                " (eléctrica)"
                            } else {
                                " (¿frigorífica? el RSIF usa la eléctrica de compresores)"
                            }
                        ),
                        fuente(&m),
                        electrica,
                    );
                }
            }
        }

        // Generadores RITE.
        if equipo
            && !es_if
            && let Some(g) = generador(&r)
        {
            let leido = frio_calor(&normalizar(&m.resumen), g)
                .or_else(|| frio_calor(&normalizar(&m.texto), g))
                .or_else(|| {
                    m.recursos
                        .iter()
                        .filter(|(n, _)| {
                            let n = normalizar(n);
                            !no_es_equipo(&n) && generador(&n).is_some()
                        })
                        .find_map(|(n, q)| frio_calor(&normalizar(n), g).map(|(f, c)| (f * q, c * q)))
                });
            match leido {
                Some((f, c)) => {
                    l.potencia_frio += f * m.cantidad;
                    l.potencia_calor += c * m.cantidad;
                    let mut d = Vec::new();
                    if !f.is_zero() {
                        d.push(format!("{} kW frío", cant(f)));
                    }
                    if !c.is_zero() {
                        d.push(format!("{} kW calor", cant(c)));
                    }
                    pista(
                        &mut l,
                        format!("Generador: {} por unidad", d.join(" / ")),
                        fuente(&m),
                        true,
                    );
                }
                None => {
                    l.sin_potencia.push(fuente(&m));
                    pista(
                        &mut l,
                        "Generador sin potencia en el texto: añádala a mano".into(),
                        fuente(&m),
                        false,
                    );
                }
            }
            if g != Generador::Calor {
                l.circuitos += entero(m.cantidad);
                modelos.insert(m.codigo.clone());
            }
        }

        // PCI (por el resumen: el texto largo menciona de todo).
        let mut sistema_de_partida = None;
        for (nombre, claves) in SISTEMAS_PCI {
            // Detección de fugas o de CO y extracción de cocinas no son PCI.
            let ajeno = (*nombre == "detección" && alguna(&t, &["fuga", "monoxido", "co2", "detector de gas"]))
                || (*nombre == "control de humos" && alguna(&t, &["cocina", "campana"]));
            if !ajeno && (alguna(&r, claves) || (*nombre == "BIE" && menciona_bie(&m.resumen))) {
                sistema_de_partida.get_or_insert(*nombre);
                if sistemas.insert(nombre) {
                    pista(&mut l, format!("PCI: {nombre}"), fuente(&m), true);
                }
            }
        }
        if equipo && sistema_de_partida.is_some() {
            if (alguna(&r, &["boca de incendio"]) || menciona_bie(&m.resumen)) && !r.contains("deteccion") {
                l.bie += entero(m.cantidad);
            }
            if r.contains("detector")
                && sistemas.contains("detección")
                && !alguna(&r, &["fuga", "monoxido", "co2", "gas"])
            {
                l.detectores += entero(m.cantidad);
            }
        }
        if alguna(&r, &["sobrepresion", "presion diferencial"]) {
            l.presion_diferencial = true;
        }
    }
    l.modelos = modelos.len() as u32;
    l.deteccion = sistemas.contains("detección");
    l.sistemas_pci = SISTEMAS_PCI
        .iter()
        .map(|s| s.0)
        .filter(|s| sistemas.contains(s))
        .collect();
    if l.bie > 0 {
        let n = l.bie;
        pista(&mut l, format!("{n} BIE: cálculo hidráulico"), String::new(), true);
    }
    if l.detectores > 0 {
        let n = l.detectores;
        pista(
            &mut l,
            format!("{n} detectores: cobertura y baterías"),
            String::new(),
            true,
        );
    }
    l
}

fn rank(r: Refrigerante) -> u8 {
    match r {
        Refrigerante::L1 => 0,
        Refrigerante::A2l => 1,
        Refrigerante::L2 => 2,
        Refrigerante::L3 => 3,
    }
}

pub fn grupo(r: Refrigerante) -> &'static str {
    match r {
        Refrigerante::L1 => "L1",
        Refrigerante::A2l => "A2L",
        Refrigerante::L2 => "L2",
        Refrigerante::L3 => "L3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppto_core::Concepto;
    use rust_decimal_macros::dec;

    fn partida(p: &mut Presupuesto, cap: &str, codigo: &str, resumen: &str, n: Decimal) {
        p.insertar(Concepto::partida(codigo, "ud", resumen, vec![])).unwrap();
        p.anadir_linea(cap, codigo, n).unwrap();
    }

    /// Supermercado sintético: clima VRF con R-32, frío comercial con CO₂ y PCI.
    fn supermercado() -> Presupuesto {
        let mut p = Presupuesto::new("sintético", "OBRA", "Supermercado de prueba");
        p.nuevo_capitulo("OBRA", "C01", "CLIMATIZACIÓN").unwrap();
        p.nuevo_capitulo("OBRA", "C02", "REFRIGERACIÓN COMERCIAL").unwrap();
        p.nuevo_capitulo("OBRA", "C03", "PROTECCIÓN CONTRA INCENDIOS").unwrap();
        partida(
            &mut p,
            "C01",
            "E01",
            "Ud. Unidad exterior VRF bomba de calor R-32, potencia frigorífica 28 kW, calorífica 31,5 kW",
            dec!(2),
        );
        partida(&mut p, "C01", "E02", "Ud. Unidad interior cassette 7,1 kW", dec!(8));
        partida(
            &mut p,
            "C01",
            "E03",
            "Ml. Tubería frigorífica de cobre para unidad exterior",
            dec!(120),
        );
        partida(&mut p, "C01", "E04", "Ud. Split pared 3,5 kW R-32", dec!(1));
        partida(
            &mut p,
            "C02",
            "F01",
            "Ud. Central frigorífica de compresores R-744, potencia absorbida 18 kW, potencia frigorífica 45 kW",
            dec!(1),
        );
        partida(
            &mut p,
            "C02",
            "F02",
            "Ud. Cámara frigorífica de conservación 20 m²",
            dec!(1),
        );
        partida(&mut p, "C03", "P01", "Ud. Extintor de polvo ABC 6 kg", dec!(10));
        partida(&mut p, "C03", "P02", "Ud. BIE 25 mm con manguera de 20 m", dec!(4));
        partida(&mut p, "C03", "P03", "Ud. Señal fotoluminiscente de BIE", dec!(4));
        partida(&mut p, "C03", "P04", "Ud. Detector óptico de humos", dec!(20));
        partida(&mut p, "C03", "P05", "Ud. Pulsador de alarma", dec!(4));
        p
    }

    #[test]
    fn lee_supermercado() {
        let l = leer_presupuesto(&supermercado());
        assert_eq!(l.capitulos_rite, ["C01", "C02"]);
        assert_eq!(l.capitulos_pci, ["C03"]);
        assert_eq!(l.potencia_frio, dec!(59.5));
        assert_eq!(l.potencia_calor, dec!(66.5));
        assert_eq!((l.circuitos, l.modelos), (3, 2));
        assert_eq!(l.refrigerante, Some(Refrigerante::A2l));
        assert!(l.frigorifica);
        assert_eq!((l.if_kw_sistema, l.if_kw_total), (dec!(18), dec!(18)));
        assert_eq!(l.registro_if(), Some(RegistroIf::MemoriaA2l));
        assert_eq!(
            l.sistemas_pci,
            ["extintores", "BIE", "detección", "alarma", "señalización"]
        );
        assert_eq!((l.bie, l.detectores), (4, 20));
        assert!(l.deteccion);
    }

    #[test]
    fn rellena_y_avisa() {
        let l = leer_presupuesto(&supermercado());
        let mut e = Expediente::nuevo(Disciplina::Pci, crate::Documento::Memoria, dec!(50000));
        l.rellenar(&mut e);
        assert_eq!(e.instalaciones_pci, 5);
        assert!(e.pci_bie && e.pci_deteccion);
        assert!(e.avisos_documentacion()[0].contains("exige proyecto"));
    }

    #[test]
    fn potencias_con_rotulos() {
        let v = |s: &str| potencias(&normalizar(s));
        assert_eq!(v("Bomba de calor 28/31,5 kW"), [(dec!(28), None), (dec!(31.5), None)]);
        assert_eq!(v("Pf = 12,5 kW; Pc = 14 kW")[1], (dec!(14), Some(Rotulo::Calor)));
        assert_eq!(v("caldera 24 kW, 30 kWh"), [(dec!(24), None)]);
        assert_eq!(v("modelo PUHZ100 de 10 kW")[0].0, dec!(10));
        assert_eq!(v("8.600 frig/h")[0].0, dec!(10));
        assert_eq!(
            frio_calor(&normalizar("Caldera de condensación 24 kW"), Generador::Calor),
            Some((dec!(0), dec!(24)))
        );
    }

    #[test]
    fn refrigerantes() {
        assert_eq!(refrigerante("gas R-410A").map(|r| r.0), Some(Refrigerante::L1));
        assert_eq!(refrigerante("R32 y R-290").map(|r| r.0), Some(Refrigerante::L3));
        assert_eq!(refrigerante("PR32 sin gas"), None);
    }
}
