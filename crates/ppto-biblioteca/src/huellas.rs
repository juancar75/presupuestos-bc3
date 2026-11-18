// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparar con obras parecidas: la «huella» de cada obra (tipo, datos del
//! edificio, importes por grupo de capítulo, reparto de costes y horas de
//! mano de obra) se guarda en la biblioteca compartida y, con ella, los
//! ratios de una obra (€/plaza, €/m², h/plaza…) se comparan con la mediana
//! de las obras del mismo tipo y tamaño parecido.

use crate::indice::normalizar as plano;
use ppto_core::Decimal;
use ppto_core::Presupuesto;
use ppto_core::concepto::Naturaleza;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Cómo acabó la oferta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    #[default]
    Ofertada,
    Adjudicada,
    Perdida,
}

impl Estado {
    pub const TODOS: [Estado; 3] = [Estado::Ofertada, Estado::Adjudicada, Estado::Perdida];

    pub fn nombre(self) -> &'static str {
        match self {
            Estado::Ofertada => "Ofertada",
            Estado::Adjudicada => "Adjudicada",
            Estado::Perdida => "Perdida",
        }
    }

    pub fn leer(s: &str) -> Option<Self> {
        Self::TODOS
            .into_iter()
            .find(|e| e.nombre().eq_ignore_ascii_case(s.trim()))
    }
}

/// Lo que se guarda de cada obra para compararla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Huella {
    /// Nombre para mostrar (la obra o su referencia).
    pub nombre: String,
    /// `AAAA-MM-DD` de la última actualización.
    pub fecha: String,
    /// Claves de especialidad (`ventilacion_garaje`, `pci`…).
    pub especialidades: Vec<String>,
    #[serde(default)]
    pub estado: Estado,
    /// Datos del edificio (ficha numérica).
    #[serde(default)]
    pub ficha: BTreeMap<String, Decimal>,
    /// PEM (con costes indirectos).
    pub pem: Decimal,
    /// Importe por grupo de capítulo (Ventilación, Detección…).
    #[serde(default)]
    pub grupos: BTreeMap<String, Decimal>,
    /// Coste por naturaleza: materiales, mano de obra, subcontratas…
    #[serde(default)]
    pub costes: BTreeMap<String, Decimal>,
    /// Horas de mano de obra por oficio (resumen del recurso).
    #[serde(default)]
    pub horas: BTreeMap<String, Decimal>,
}

/// Grupos de capítulo por palabras de su título (en orden: la primera que
/// casa manda).
const GRUPOS: &[(&str, &[&str])] = &[
    ("Detección", &["DETECC", "ALARMA", "MONOXIDO", "CENTRAL"]),
    ("Extintores", &["EXTINTOR"]),
    ("BIE", &["BIE", "BOCA DE INCENDIO", "BOCAS DE INCENDIO"]),
    ("Rociadores", &["ROCIAD", "SPRINK"]),
    ("Columna seca", &["COLUMNA SECA"]),
    (
        "Abastecimiento PCI",
        &["GRUPO DE PRESION", "ALJIBE", "ABASTEC", "DEPOSITO"],
    ),
    ("Señalización y alumbrado", &["SENALIZ", "ALUMBRADO"]),
    (
        "Ventilación",
        &["VENTILA", "EXTRACC", "IMPULSION", "SOBREPRESION", "PRESURIZ"],
    ),
    ("Conductos y difusión", &["CONDUCTO", "REJILLA", "DIFUSOR", "DIFUSION"]),
    (
        "Producción",
        &[
            "PRODUCCION",
            "AEROTERM",
            "BOMBA DE CALOR",
            "VRF",
            "VRV",
            "ENFRIADORA",
            "CALDERA",
        ],
    ),
    (
        "Unidades terminales",
        &["UNIDADES INTERIORES", "FANCOIL", "TERMINAL", "SPLIT"],
    ),
    ("Distribución", &["DISTRIBUC", "TUBER", "SUELO RADIANTE", "FRIGORIF"]),
    ("ACS", &["ACS", "AGUA CALIENTE"]),
    ("Saneamiento", &["SANEAM"]),
    ("Fontanería", &["FONTANER", "ACOMETIDA", "CONTADOR"]),
    ("Control", &["CONTROL", "REGULACION", "BMS", "GESTION"]),
    ("Electricidad", &["ELECTRIC", "CUADRO", "CABLEADO"]),
    (
        "Legalización",
        &["LEGALIZ", "PROYECTO", "CERTIFIC", "TRAMITAC", "DOCUMENTAC"],
    ),
    (
        "Ayudas y varios",
        &["AYUDAS", "VARIOS", "MEDIOS AUX", "SEGURIDAD", "GESTION DE RESIDUOS"],
    ),
];

/// Grupo de un capítulo por su título.
pub fn grupo_de(titulo: &str) -> Option<&'static str> {
    let t = format!(" {} ", plano(titulo));
    GRUPOS
        .iter()
        .find(|(_, ps)| {
            ps.iter().any(|p| {
                // Palabras cortas («BIE», «ACS», «VRF»), como palabra entera
                if p.len() <= 4 {
                    t.contains(&format!(" {p} "))
                } else {
                    t.contains(p)
                }
            })
        })
        .map(|(g, _)| *g)
}

fn sumar_grupos(
    p: &Presupuesto,
    precios: &std::collections::HashMap<String, Decimal>,
    cap: &str,
    heredado: Option<&'static str>,
    factor: Decimal,
    out: &mut BTreeMap<String, Decimal>,
    profundidad: u8,
) {
    let Some(c) = p.conceptos.get(cap) else { return };
    if profundidad > 12 {
        return;
    }
    let g = grupo_de(&c.resumen).or(heredado);
    for l in &c.descomposicion {
        let Some(h) = p.conceptos.get(&l.hijo) else { continue };
        let f = factor * l.rendimiento;
        if h.naturaleza == Naturaleza::Capitulo {
            sumar_grupos(p, precios, &l.hijo, g, f, out, profundidad + 1);
        } else if !h.fuera_de_importe() {
            let importe = precios.get(&l.hijo).copied().unwrap_or_default() * f;
            if !importe.is_zero() {
                let g = grupo_de(&h.resumen).filter(|_| g.is_none()).or(g).unwrap_or("Otros");
                *out.entry(g.to_owned()).or_default() += importe;
            }
        }
    }
}

fn es_hora(u: &str) -> bool {
    matches!(u.trim().to_lowercase().as_str(), "h" | "hr" | "hora" | "horas")
}

/// Huella de una obra.
pub fn huella(
    p: &Presupuesto,
    nombre: &str,
    especialidades: &[String],
    ficha: &BTreeMap<String, Decimal>,
    estado: Estado,
    fecha: &str,
) -> Result<Huella, String> {
    let v = p.valorar().map_err(|e| e.to_string())?;
    let mut grupos = BTreeMap::new();
    sumar_grupos(p, &v.precios, &p.raiz, None, Decimal::ONE, &mut grupos, 0);
    let mut costes: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut horas: BTreeMap<String, Decimal> = BTreeMap::new();
    if let Ok(e) = p.explosion_recursos() {
        for r in &e.recursos {
            let n = match r.naturaleza {
                Naturaleza::Material => "Materiales",
                Naturaleza::ManoObra => "Mano de obra",
                Naturaleza::Subcontrata => "Subcontratas",
                Naturaleza::Maquinaria => "Maquinaria",
                _ => "Otros",
            };
            *costes.entry(n.to_owned()).or_default() += r.importe;
            if r.naturaleza == Naturaleza::ManoObra && es_hora(&r.unidad) {
                *horas.entry(r.resumen.clone()).or_default() += r.cantidad;
            }
        }
    }
    let r2 = |x: Decimal| x.round_dp(2);
    Ok(Huella {
        nombre: nombre.to_owned(),
        fecha: fecha.to_owned(),
        especialidades: especialidades.to_vec(),
        estado,
        ficha: ficha.clone(),
        pem: r2(v.pem),
        grupos: grupos.into_iter().map(|(k, x)| (k, r2(x))).collect(),
        costes: costes.into_iter().map(|(k, x)| (k, r2(x))).collect(),
        horas: horas.into_iter().map(|(k, x)| (k, r2(x))).collect(),
    })
}

/// Una unidad de comparación: «plaza», «m² construido»…
#[derive(Debug, Clone, Copy)]
pub struct Medida {
    pub nombre: &'static str,
    /// Calcula la medida con la ficha (o `None` si faltan datos).
    pub de: fn(&BTreeMap<String, Decimal>) -> Option<Decimal>,
}

fn dato(f: &BTreeMap<String, Decimal>, k: &str) -> Option<Decimal> {
    f.get(k).copied().filter(|x| *x > Decimal::ZERO)
}

const PLAZA: Medida = Medida {
    nombre: "plaza",
    de: |f| dato(f, "garaje_plazas"),
};
const M2_GARAJE: Medida = Medida {
    nombre: "m² de garaje",
    de: |f| Some(dato(f, "garaje_superficie_planta")? * dato(f, "plantas_bajo").unwrap_or(Decimal::ONE)),
};
const M2_CONSTRUIDO: Medida = Medida {
    nombre: "m² construido",
    de: |f| dato(f, "superficie_construida"),
};
const M2_UTIL: Medida = Medida {
    nombre: "m² útil",
    de: |f| dato(f, "superficie_util"),
};
const VIVIENDA: Medida = Medida {
    nombre: "vivienda",
    de: |f| dato(f, "viviendas"),
};

/// Medidas con las que se compara cada tipo de obra (la primera es la
/// principal: con ella se decide qué obras son de tamaño parecido).
pub fn medidas(especialidades: &[String]) -> Vec<Medida> {
    let tiene = |k: &str| especialidades.iter().any(|e| e == k);
    let mut v = Vec::new();
    if tiene("ventilacion_garaje") {
        v.extend([PLAZA, M2_GARAJE]);
    }
    if tiene("pci") {
        v.extend([M2_CONSTRUIDO, VIVIENDA, PLAZA]);
    }
    if tiene("climatizacion") || tiene("ventilacion_viviendas") || tiene("fontaneria") {
        v.extend([VIVIENDA, M2_UTIL]);
    }
    if v.is_empty() {
        v.extend([M2_CONSTRUIDO, VIVIENDA]);
    }
    let mut vistos = Vec::new();
    v.retain(|m| {
        let nuevo = !vistos.contains(&m.nombre);
        vistos.push(m.nombre);
        nuevo
    });
    v
}

/// Un ratio de la obra frente a las parecidas.
#[derive(Debug, Clone, PartialEq)]
pub struct Ratio {
    /// «PEM €/plaza», «Ventilación €/plaza», «h de mano de obra/plaza»…
    pub nombre: String,
    pub esta: Option<Decimal>,
    pub mediana: Option<Decimal>,
    pub minimo: Option<Decimal>,
    pub maximo: Option<Decimal>,
    /// Obras parecidas con este ratio.
    pub n: usize,
    /// (esta − mediana) / mediana, en %.
    pub desviacion: Option<Decimal>,
}

/// Resultado de comparar una obra con las parecidas.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparacion {
    /// Obras usadas: clave, nombre, fecha, estado, tamaño en la medida principal.
    pub parecidas: Vec<(String, String, String, Estado, Option<Decimal>)>,
    /// Medida principal.
    pub medida: Option<&'static str>,
    /// Se amplió el margen de tamaño por haber pocas obras parecidas.
    pub ampliado: bool,
    /// Solo adjudicadas (si hay al menos 3).
    pub solo_adjudicadas: bool,
    pub ratios: Vec<Ratio>,
    /// Qué falta para comparar (sin ficha, sin especialidad…).
    pub avisos: Vec<String>,
}

fn mediana(v: &mut [Decimal]) -> Option<Decimal> {
    if v.is_empty() {
        return None;
    }
    v.sort();
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / Decimal::TWO
    })
}

fn horas_total(h: &Huella) -> Decimal {
    h.horas.values().copied().sum()
}

/// Ratios de una huella (nombre → valor) con unas medidas.
fn ratios_de(h: &Huella, medidas: &[Medida]) -> BTreeMap<String, Decimal> {
    let mut r = BTreeMap::new();
    for (i, m) in medidas.iter().enumerate() {
        let Some(q) = (m.de)(&h.ficha) else { continue };
        r.insert(format!("PEM €/{}", m.nombre), h.pem / q);
        let horas = horas_total(h);
        if !horas.is_zero() {
            r.insert(format!("h de mano de obra/{}", m.nombre), horas / q);
        }
        // Grupos y costes, solo con la medida principal (para no repetir)
        if i == 0 {
            for (g, x) in &h.grupos {
                r.insert(format!("{g} €/{}", m.nombre), *x / q);
            }
            for (c, x) in &h.costes {
                r.insert(format!("{c} €/{}", m.nombre), *x / q);
            }
        }
    }
    r
}

/// Compara `esta` con las huellas de la biblioteca del mismo tipo y tamaño
/// parecido (la mitad al doble en la medida principal; si hay menos de 3,
/// todas las del mismo tipo). Si hay 3 o más adjudicadas, solo esas.
pub fn comparar(clave: &str, esta: &Huella, todas: &BTreeMap<String, Huella>) -> Comparacion {
    let mut c = Comparacion::default();
    if esta.especialidades.is_empty() {
        c.avisos
            .push("La obra no tiene especialidad: elíjala para saber con qué obras compararla.".into());
        return c;
    }
    let medidas = medidas(&esta.especialidades);
    let principal = medidas.first().copied();
    c.medida = principal.map(|m| m.nombre);
    let tam_esta = principal.and_then(|m| (m.de)(&esta.ficha));
    if tam_esta.is_none() {
        c.avisos.push(format!(
            "Falta el dato de «{}» en Datos de la obra: sin él no hay ratios.",
            principal.map_or("tamaño", |m| m.nombre)
        ));
    }
    let mismo_tipo: Vec<(&String, &Huella)> = todas
        .iter()
        .filter(|(k, h)| *k != clave && h.especialidades.iter().any(|e| esta.especialidades.contains(e)))
        .collect();
    let tam = |h: &Huella| principal.and_then(|m| (m.de)(&h.ficha));
    let mut elegidas: Vec<(&String, &Huella)> = match tam_esta {
        Some(t) => mismo_tipo
            .iter()
            .filter(|(_, h)| tam(h).is_some_and(|x| x * Decimal::TWO >= t && x <= t * Decimal::TWO))
            .copied()
            .collect(),
        None => mismo_tipo.clone(),
    };
    if elegidas.len() < 3 && mismo_tipo.len() > elegidas.len() {
        elegidas = mismo_tipo.clone();
        c.ampliado = true;
    }
    let adjudicadas: Vec<(&String, &Huella)> = elegidas
        .iter()
        .filter(|(_, h)| h.estado == Estado::Adjudicada)
        .copied()
        .collect();
    if adjudicadas.len() >= 3 {
        elegidas = adjudicadas;
        c.solo_adjudicadas = true;
    }
    if elegidas.is_empty() {
        c.avisos
            .push("Aún no hay obras del mismo tipo en la biblioteca: al guardar cada obra se añade su huella.".into());
    } else if elegidas.len() < 3 {
        c.avisos.push(format!(
            "Solo {} obras del mismo tipo: la comparación es orientativa.",
            elegidas.len()
        ));
    }
    c.parecidas = elegidas
        .iter()
        .map(|(k, h)| ((*k).clone(), h.nombre.clone(), h.fecha.clone(), h.estado, tam(h)))
        .collect();
    let mias = ratios_de(esta, &medidas);
    let suyas: Vec<BTreeMap<String, Decimal>> = elegidas.iter().map(|(_, h)| ratios_de(h, &medidas)).collect();
    let mut nombres: Vec<String> = mias.keys().cloned().collect();
    for s in &suyas {
        for k in s.keys() {
            if !nombres.contains(k) {
                nombres.push(k.clone());
            }
        }
    }
    let r2 = |x: Decimal| x.round_dp(2);
    for n in nombres {
        let mut vals: Vec<Decimal> = suyas.iter().filter_map(|s| s.get(&n).copied()).collect();
        let e = mias.get(&n).copied();
        let med = mediana(&mut vals);
        let desviacion = match (e, med) {
            (Some(e), Some(m)) if !m.is_zero() => Some(((e - m) / m * Decimal::ONE_HUNDRED).round_dp(0)),
            _ => None,
        };
        c.ratios.push(Ratio {
            nombre: n,
            esta: e.map(r2),
            mediana: med.map(r2),
            minimo: vals.first().copied().map(r2),
            maximo: vals.last().copied().map(r2),
            n: vals.len(),
            desviacion,
        });
    }
    // Primero el PEM, luego las horas, luego los grupos con más desviación
    c.ratios.sort_by_key(|r| {
        let orden = if r.nombre.starts_with("PEM") {
            0
        } else if r.nombre.starts_with("h de") {
            1
        } else {
            2
        };
        (
            orden,
            std::cmp::Reverse(r.desviacion.map(|d| d.abs()).unwrap_or_default()),
        )
    });
    c
}

impl Comparacion {
    /// Texto para Claude o para copiar.
    pub fn texto(&self, limite_pct: Decimal) -> String {
        use std::fmt::Write as _;
        let mut t = String::from("Comparación con obras parecidas");
        if let Some(m) = self.medida {
            let _ = write!(t, " (tamaño por {m})");
        }
        let _ = writeln!(
            t,
            ": {} obras{}{}.",
            self.parecidas.len(),
            if self.solo_adjudicadas {
                ", solo adjudicadas"
            } else {
                ""
            },
            if self.ampliado {
                ", ampliado a todas las del mismo tipo"
            } else {
                ""
            }
        );
        for a in &self.avisos {
            let _ = writeln!(t, "- AVISO: {a}");
        }
        if !self.parecidas.is_empty() {
            t.push_str("Obras: ");
            t.push_str(
                &self
                    .parecidas
                    .iter()
                    .map(|(_, n, f, e, _)| format!("{n} ({f}, {})", e.nombre().to_lowercase()))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            t.push('\n');
        }
        t.push_str("\n| Ratio | Esta obra | Mediana | Mín.–máx. | Desv. |\n|---|---:|---:|---:|---:|\n");
        let f = |x: Option<Decimal>| x.map_or("—".to_owned(), |x| ppto_core::formato::num(x, 2));
        for r in &self.ratios {
            let marca = r.desviacion.is_some_and(|d| d.abs() > limite_pct);
            let _ = writeln!(
                t,
                "| {}{} | {} | {} | {}–{} | {} |",
                if marca { "⚠ " } else { "" },
                r.nombre,
                f(r.esta),
                f(r.mediana),
                f(r.minimo),
                f(r.maximo),
                r.desviacion.map_or("—".to_owned(), |d| format!("{d:+} %"))
            );
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn h(nombre: &str, plazas: Decimal, pem: Decimal, vent: Decimal, estado: Estado) -> Huella {
        Huella {
            nombre: nombre.into(),
            fecha: "2026-01-01".into(),
            especialidades: vec!["ventilacion_garaje".into()],
            estado,
            ficha: BTreeMap::from([("garaje_plazas".into(), plazas)]),
            pem,
            grupos: BTreeMap::from([("Ventilación".into(), vent)]),
            costes: BTreeMap::new(),
            horas: BTreeMap::from([("Oficial".into(), plazas)]),
        }
    }

    #[test]
    fn grupos_por_titulo() {
        assert_eq!(grupo_de("03 DETECCIÓN DE CO"), Some("Detección"));
        assert_eq!(grupo_de("Bocas de incendio equipadas (BIE)"), Some("BIE"));
        assert_eq!(grupo_de("Ventilación forzada de garaje"), Some("Ventilación"));
        assert_eq!(grupo_de("Legalización"), Some("Legalización"));
        assert_eq!(grupo_de("ABIERTOS"), None, "BIE solo como palabra");
    }

    #[test]
    fn huella_del_ejemplo() {
        let p = ppto_core::ejemplos::suelo_radiante();
        let x = huella(
            &p,
            "Ejemplo",
            &["climatizacion".into()],
            &BTreeMap::new(),
            Estado::Ofertada,
            "2026-11-18",
        )
        .unwrap();
        assert!(x.pem > Decimal::ZERO);
        let suma: Decimal = x.grupos.values().copied().sum();
        // Los grupos suman el coste directo (sin indirectos), con redondeos
        assert!(
            (suma - p.valorar().unwrap().coste_directo).abs() < dec!(1),
            "{:?}",
            x.grupos
        );
        assert!(x.costes.contains_key("Materiales"));
        assert!(!x.horas.is_empty());
    }

    #[test]
    fn compara_con_las_parecidas() {
        let mut todas = BTreeMap::new();
        todas.insert(
            "a".to_owned(),
            h("A", dec!(50), dec!(50000), dec!(30000), Estado::Adjudicada),
        );
        todas.insert(
            "b".to_owned(),
            h("B", dec!(60), dec!(66000), dec!(36000), Estado::Ofertada),
        );
        todas.insert(
            "c".to_owned(),
            h("C", dec!(80), dec!(96000), dec!(48000), Estado::Perdida),
        );
        // Muy grande: fuera de tamaño
        todas.insert(
            "d".to_owned(),
            h("D", dec!(500), dec!(300000), dec!(200000), Estado::Adjudicada),
        );
        let mut otra = h("PCI", dec!(60), dec!(1), dec!(1), Estado::Adjudicada);
        otra.especialidades = vec!["pci".into()];
        todas.insert("e".to_owned(), otra);
        let esta = h("Esta", dec!(60), dec!(90000), dec!(54000), Estado::Ofertada);
        todas.insert("esta".to_owned(), esta.clone());
        let c = comparar("esta", &esta, &todas);
        assert_eq!(c.parecidas.len(), 3, "{:?}", c.parecidas);
        assert!(!c.ampliado && !c.solo_adjudicadas);
        let r = c.ratios.iter().find(|r| r.nombre == "PEM €/plaza").unwrap();
        // A 1000, B 1100, C 1200 → mediana 1100; esta 1500 → +36 %
        assert_eq!(r.mediana, Some(dec!(1100)));
        assert_eq!(r.esta, Some(dec!(1500)));
        assert_eq!(r.desviacion, Some(dec!(36)));
        assert_eq!((r.minimo, r.maximo), (Some(dec!(1000)), Some(dec!(1200))));
        assert!(c.ratios.iter().any(|r| r.nombre == "Ventilación €/plaza"));
        assert!(c.ratios.iter().any(|r| r.nombre == "h de mano de obra/plaza"));
        assert!(
            c.texto(dec!(25)).contains("| ⚠ PEM €/plaza | 1.500,00 | 1.100,00 |"),
            "{}",
            c.texto(dec!(25))
        );
        // Sin plazas en la ficha: aviso, y se compara con todas las del tipo
        let mut sin = esta.clone();
        sin.ficha.clear();
        let c = comparar("esta", &sin, &todas);
        assert!(c.avisos.iter().any(|a| a.contains("plaza")), "{:?}", c.avisos);
        assert_eq!(c.parecidas.len(), 4);
    }
}
