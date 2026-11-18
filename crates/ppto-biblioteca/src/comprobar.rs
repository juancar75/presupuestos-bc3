// SPDX-License-Identifier: GPL-3.0-or-later
//! Comprobación antes de valorar: leer la obra y ver si están todos los
//! recursos para sacar los precios (tarifas, descuentos, nombres de
//! fabricante) y avisar de los huecos al empezar.

use crate::Biblioteca;
use crate::carpeta::{self, Carpeta};
use crate::indice::{contiene, normalizar as plano};
use ppto_core::concepto::Naturaleza;
use ppto_core::descuentos::normalizar;
use ppto_core::{Decimal, Presupuesto};
use std::collections::{BTreeMap, BTreeSet};

/// Estado de un fabricante de la obra.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EstadoFabricante {
    pub nombre: String,
    /// Recursos de la obra con este fabricante.
    pub recursos: usize,
    /// Fecha (`AAAA-MM`) de su tarifa vigente en la carpeta de tarifas.
    pub tarifa_carpeta: Option<String>,
    /// Etiqueta de su tarifa más reciente cargada en la biblioteca.
    pub tarifa_biblioteca: Option<String>,
    /// Tiene descuento en la obra (general o de algún grupo).
    pub dto_obra: bool,
    /// Tiene descuento en la biblioteca.
    pub dto_biblioteca: bool,
    /// Nombres parecidos en la biblioteca o la carpeta (¿el mismo fabricante
    /// con otro nombre?).
    pub parecidos: Vec<String>,
}

/// Resultado de la comprobación.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Comprobacion {
    pub partidas: usize,
    pub sin_precio: usize,
    pub fabricantes: Vec<EstadoFabricante>,
    /// Materiales con precio pero sin tarifa (neto fijo): código, resumen.
    pub netos_fijos: Vec<(String, String)>,
    /// Recursos a 0 €: código, resumen.
    pub a_cero: Vec<(String, String)>,
    /// PVP de la obra distinto del de la biblioteca: código, obra, biblioteca.
    pub pvp_distinto: Vec<(String, Decimal, Decimal)>,
    /// Marcas conocidas que se nombran en partidas sin precio: marca,
    /// partidas, ¿tiene tarifa (carpeta o biblioteca)?, ¿tiene descuento?
    pub marcas: Vec<(String, usize, bool, bool)>,
}

/// ¿Se parecen dos nombres de fabricante? (uno contiene al otro, o empiezan
/// por la misma palabra de 4 letras o más)
fn parecido(a: &str, b: &str) -> bool {
    let (a, b) = (plano(a), plano(b));
    if a == b || a.is_empty() || b.is_empty() {
        return false;
    }
    let primera = |s: &str| s.split(' ').next().unwrap_or("").to_owned();
    (a.len() >= 3 && contiene(&b, &a))
        || (b.len() >= 3 && contiene(&a, &b))
        || (primera(&a).len() >= 4 && primera(&a) == primera(&b))
}

/// Comprueba la obra con la biblioteca y la carpeta de tarifas.
pub fn comprobar(p: &Presupuesto, b: &Biblioteca, carpeta: Option<&Carpeta>) -> Comprobacion {
    let mut c = Comprobacion::default();
    let precios = p.valorar().map(|v| v.precios).unwrap_or_default();
    let cant = p.cantidades_partidas().unwrap_or_default();
    let sin_precio: Vec<&String> = cant
        .keys()
        .filter(|k| {
            let x = &p.conceptos[*k];
            !x.fuera_de_importe() && precios.get(*k).is_none_or(Decimal::is_zero)
        })
        .collect();
    c.partidas = cant.len();
    c.sin_precio = sin_precio.len();

    let en_carpeta: BTreeMap<String, String> = carpeta
        .map(|k| {
            k.fabricantes
                .iter()
                .filter_map(|(f, d)| Some((carpeta::nombre_carpeta(f), d.first()?.fecha.clone())))
                .collect()
        })
        .unwrap_or_default();
    let conocidos: BTreeSet<String> = b
        .fabricantes
        .keys()
        .cloned()
        .chain(en_carpeta.keys().cloned())
        .collect();

    // Recursos
    let mut por_fab: BTreeMap<String, usize> = BTreeMap::new();
    for x in p.conceptos.values() {
        let recurso = matches!(
            x.naturaleza,
            Naturaleza::Material
                | Naturaleza::ManoObra
                | Naturaleza::Maquinaria
                | Naturaleza::Subcontrata
                | Naturaleza::Otros
        );
        if !recurso {
            continue;
        }
        let precio = precios.get(&x.codigo).copied().unwrap_or_default();
        if precio.is_zero() {
            c.a_cero.push((x.codigo.clone(), x.resumen.clone()));
        }
        match &x.tarifa {
            Some(t) => {
                *por_fab.entry(normalizar(&t.fabricante)).or_default() += 1;
                if let Some(r) = Presupuesto::referencia_tarifa(x)
                    && let Some(pvp) = b
                        .fabricante(&t.fabricante)
                        .and_then(|f| f.tarifas.iter().rev().find(|y| y.lineas.contains_key(&r)))
                        .map(|y| y.lineas[&r].pvp)
                    && pvp != t.pvp
                {
                    c.pvp_distinto.push((x.codigo.clone(), t.pvp, pvp));
                }
            }
            None if x.naturaleza == Naturaleza::Material && !precio.is_zero() => {
                c.netos_fijos.push((x.codigo.clone(), x.resumen.clone()));
            }
            None => {}
        }
    }
    let dto_obra = |f: &str| {
        p.descuentos
            .keys()
            .chain(p.descuentos_encadenados.keys())
            .any(|k| normalizar(k) == f || normalizar(k).starts_with(&format!("{f}/")))
    };
    for (f, n) in por_fab {
        let fb = b.fabricante(&f);
        let en_bib = fb.is_some();
        let en_carp = en_carpeta.get(&carpeta::nombre_carpeta(&f)).cloned();
        let parecidos = if en_bib && en_carp.is_some() {
            Vec::new()
        } else {
            conocidos.iter().filter(|k| parecido(&f, k)).cloned().collect()
        };
        c.fabricantes.push(EstadoFabricante {
            recursos: n,
            tarifa_carpeta: en_carp,
            tarifa_biblioteca: fb.and_then(|x| x.tarifas.last()).map(|t| t.etiqueta.clone()),
            dto_obra: dto_obra(&f),
            dto_biblioteca: fb.is_some_and(|x| !x.descuentos.is_empty()),
            parecidos,
            nombre: f,
        });
    }

    // Marcas conocidas nombradas en las partidas sin precio
    let textos: Vec<String> = sin_precio
        .iter()
        .map(|k| {
            let x = &p.conceptos[*k];
            plano(&format!("{} {}", x.resumen, x.texto.as_deref().unwrap_or("")))
        })
        .collect();
    for m in &conocidos {
        let mp = plano(m);
        if mp.len() < 2 {
            continue;
        }
        let n = textos.iter().filter(|t| contiene(t, &mp)).count();
        if n > 0 {
            let fb = b.fabricante(m);
            let tarifa =
                en_carpeta.contains_key(&carpeta::nombre_carpeta(m)) || fb.is_some_and(|x| !x.tarifas.is_empty());
            let dto = dto_obra(m) || fb.is_some_and(|x| !x.descuentos.is_empty());
            c.marcas.push((m.clone(), n, tarifa, dto));
        }
    }
    c.marcas.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    c
}

/// Gravedad de un aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Nivel {
    /// Falta algo para valorar bien.
    Falta,
    /// Revisar.
    Revisar,
    Bien,
}

impl Comprobacion {
    /// Avisos en lenguaje llano, de lo más grave a lo menos.
    pub fn avisos(&self) -> Vec<(Nivel, String)> {
        let mut v = Vec::new();
        for f in &self.fabricantes {
            let mut falta = Vec::new();
            if f.tarifa_carpeta.is_none() && f.tarifa_biblioteca.is_none() {
                falta.push("sin tarifa (ni en la carpeta ni en la biblioteca)".to_owned());
            }
            if !f.dto_obra && !f.dto_biblioteca {
                falta.push("sin descuento".to_owned());
            } else if !f.dto_obra {
                falta.push("la obra no tiene su descuento (está en la biblioteca)".to_owned());
            }
            if !f.parecidos.is_empty() {
                falta.push(format!(
                    "¿es el mismo que {}? Use el mismo nombre",
                    f.parecidos.join(", ")
                ));
            }
            if !falta.is_empty() {
                let nivel = if f.tarifa_carpeta.is_none() && f.tarifa_biblioteca.is_none()
                    || !f.dto_obra && !f.dto_biblioteca
                {
                    Nivel::Falta
                } else {
                    Nivel::Revisar
                };
                v.push((
                    nivel,
                    format!("{} ({} recursos): {}.", f.nombre, f.recursos, falta.join("; ")),
                ));
            }
        }
        for (m, n, tarifa, dto) in &self.marcas {
            if !tarifa || !dto {
                let que = match (tarifa, dto) {
                    (false, false) => "no hay tarifa ni descuento",
                    (false, true) => "no hay tarifa",
                    _ => "no hay descuento",
                };
                v.push((
                    Nivel::Falta,
                    format!("{m} se nombra en {n} partidas sin precio y {que}."),
                ));
            }
        }
        if !self.pvp_distinto.is_empty() {
            v.push((
                Nivel::Revisar,
                format!(
                    "{} materiales con PVP distinto del de la biblioteca ({}).",
                    self.pvp_distinto.len(),
                    lista(self.pvp_distinto.iter().map(|x| x.0.clone()))
                ),
            ));
        }
        if !self.netos_fijos.is_empty() {
            v.push((
                Nivel::Revisar,
                format!(
                    "{} materiales con neto fijo, sin tarifa ni descuento a la vista ({}).",
                    self.netos_fijos.len(),
                    lista(self.netos_fijos.iter().map(|x| x.0.clone()))
                ),
            ));
        }
        if !self.a_cero.is_empty() {
            v.push((
                Nivel::Revisar,
                format!(
                    "{} recursos a 0 € ({}).",
                    self.a_cero.len(),
                    lista(self.a_cero.iter().map(|x| x.0.clone()))
                ),
            ));
        }
        for (m, n, tarifa, dto) in &self.marcas {
            if *tarifa && *dto {
                v.push((
                    Nivel::Bien,
                    format!("{m}: tarifa y descuento para {n} partidas sin precio."),
                ));
            }
        }
        for f in &self.fabricantes {
            if (f.tarifa_carpeta.is_some() || f.tarifa_biblioteca.is_some()) && f.dto_obra && f.parecidos.is_empty() {
                let t = f
                    .tarifa_carpeta
                    .clone()
                    .or_else(|| f.tarifa_biblioteca.clone())
                    .unwrap_or_default();
                v.push((Nivel::Bien, format!("{}: tarifa {t} y descuento.", f.nombre)));
            }
        }
        v.sort_by_key(|x| x.0);
        v
    }

    /// Texto para Claude o para copiar.
    pub fn texto(&self) -> String {
        let mut t = format!(
            "Comprobación antes de valorar: {} partidas, {} sin precio.\n",
            self.partidas, self.sin_precio
        );
        for (n, a) in self.avisos() {
            let m = match n {
                Nivel::Falta => "FALTA",
                Nivel::Revisar => "REVISAR",
                Nivel::Bien => "OK",
            };
            t += &format!("- [{m}] {a}\n");
        }
        t
    }
}

fn lista(i: impl Iterator<Item = String>) -> String {
    let v: Vec<String> = i.collect();
    if v.len() > 6 {
        format!("{}…", v[..6].join(", "))
    } else {
        v.join(", ")
    }
}

/// Comprueba con la carpeta de tarifas configurada.
pub fn comprobar_con_carpeta(p: &Presupuesto, b: &Biblioteca) -> Comprobacion {
    let c = carpeta::ruta_carpeta().map(|r| carpeta::explorar(&r));
    comprobar(p, b, c.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carpeta::DocTarifa;
    use rust_decimal_macros::dec;

    /// Obra sintética: materiales de tres fabricantes y dos partidas sin precio.
    fn obra() -> Presupuesto {
        let mut p = Presupuesto::new("prueba", "R", "Obra sintética");
        p.nuevo_capitulo("R", "C1", "Capítulo").unwrap();
        for (c, f) in [("M.A", "FABA"), ("M.B", "FABB"), ("M.C", "SALTOKI")] {
            p.nuevo_recurso(c, "ud", c, Naturaleza::Material, dec!(0)).unwrap();
            p.fijar_tarifa_grupo(c, f, None, dec!(100)).unwrap();
        }
        p.fijar_referencia_tarifa("M.A", Some("R1")).unwrap();
        p.nuevo_recurso("M.NETO", "ud", "Neto fijo", Naturaleza::Material, dec!(7))
            .unwrap();
        p.fijar_descuento("FABA", dec!(30)).unwrap();
        p.nueva_partida("C1", "P1", "ud", "Partida con M.A").unwrap();
        for r in ["M.A", "M.B", "M.C", "M.NETO"] {
            p.anadir_linea("P1", r, dec!(1)).unwrap();
        }
        p.nueva_partida("C1", "P2", "ud", "Compuerta cortafuegos MARCAX 300x200")
            .unwrap();
        p.nueva_partida("C1", "P3", "ud", "Rejilla MARCAX y difusor").unwrap();
        p
    }

    #[test]
    fn avisa_de_lo_que_falta_para_valorar() {
        let mut b = Biblioteca::default();
        b.fijar_descuento("FABB", Some(dec!(40)));
        b.fijar_descuento("SALTOKI ONLINE", Some(dec!(45)));
        b.cargar_tarifa(
            "FABA",
            "2026",
            2026,
            BTreeMap::from([(
                "R1".into(),
                crate::LineaPvp {
                    descripcion: "x".into(),
                    unidad: "ud".into(),
                    pvp: dec!(110),
                    grupo: None,
                },
            )]),
        );
        let carpeta = Carpeta {
            fabricantes: vec![(
                "MARCAX".into(),
                vec![DocTarifa {
                    fabricante: "MARCAX".into(),
                    fecha: "2026-05".into(),
                    ruta: "MARCAX/2026-05 Tarifa MARCAX.pdf".into(),
                }],
            )],
            mal: Vec::new(),
        };
        let c = comprobar(&obra(), &b, Some(&carpeta));
        assert_eq!((c.partidas, c.sin_precio), (3, 2));
        let f = |n: &str| c.fabricantes.iter().find(|x| x.nombre == n).unwrap().clone();
        assert!(f("FABA").dto_obra && f("FABA").tarifa_biblioteca.is_some());
        assert!(!f("FABB").dto_obra && f("FABB").dto_biblioteca);
        assert_eq!(f("SALTOKI").parecidos, vec!["SALTOKI ONLINE".to_owned()]);
        assert_eq!(c.pvp_distinto, vec![("M.A".into(), dec!(100), dec!(110))]);
        assert_eq!(c.netos_fijos.len(), 1);
        assert_eq!(c.marcas, vec![("MARCAX".into(), 2, true, false)]);
        let t = c.texto();
        assert!(
            t.contains("[FALTA] MARCAX se nombra en 2 partidas sin precio y no hay descuento"),
            "{t}"
        );
        assert!(t.contains("¿es el mismo que SALTOKI ONLINE?"), "{t}");
        assert!(t.contains("[FALTA] FABB (1 recursos): sin tarifa (ni en la carpeta ni en la biblioteca); la obra no tiene su descuento"), "{t}");
        assert!(t.contains("[OK] FABA: tarifa 2026 y descuento."), "{t}");
        // Lo más grave primero
        assert!(t.find("[FALTA]").unwrap() < t.find("[REVISAR]").unwrap());
    }

    #[test]
    fn nombres_parecidos() {
        assert!(parecido("SALTOKI", "SALTOKI ONLINE"));
        assert!(parecido("MITSUBISHI ELECTRIC", "MITSUBISHI"));
        assert!(!parecido("S&P", "SP"));
        assert!(!parecido("TROX", "KOOLAIR"));
    }
}
