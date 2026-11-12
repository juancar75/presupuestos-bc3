// SPDX-License-Identifier: GPL-3.0-or-later
//! Biblioteca compartida entre varias personas.
//!
//! En una carpeta compartida (OneDrive, Teams, NAS) cada persona escribe solo
//! su fichero, `biblioteca.<usuario>.json`, y nunca el de otra: así no hay
//! copias en conflicto. Al leer se juntan todos los `biblioteca*.json` de la
//! carpeta y, para cada dato (descuento de un grupo, PVP de una referencia
//! en una tarifa, oferta, datos del fabricante, subida estimada,
//! equivalencia), **gana el cambio más reciente**, que lleva un sello con la
//! fecha y hora y quién lo hizo. Las versiones de partidas se suman.

use crate::{Biblioteca, DatosFabricante, Fabricante, LineaPvp, Oferta, TarifaFabricante};
use ppto_core::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Cuándo y quién cambió un dato.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Sello {
    /// `AAAA-MM-DD HH:MM:SS.mmm` (UTC).
    pub cuando: String,
    pub quien: String,
}

/// Fecha y hora actuales `AAAA-MM-DD HH:MM:SS.mmm` (UTC).
pub fn ahora() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let s = d.as_secs();
    let dia = crate::fecha_de(i64::try_from(s / 86_400).unwrap_or(0));
    let r = s % 86_400;
    format!(
        "{dia} {:02}:{:02}:{:02}.{:03}",
        r / 3600,
        r % 3600 / 60,
        r % 60,
        d.subsec_millis()
    )
}

/// Nombre de quien usa el programa (usuario de Windows o del sistema), solo
/// con letras, números, punto, guion y guion bajo.
pub fn usuario() -> String {
    let u = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let u: String = u
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if u.is_empty() { "usuario".into() } else { u }
}

/// Fichero propio de `usuario` en la carpeta compartida.
pub fn fichero_de(carpeta: &Path, usuario: &str) -> PathBuf {
    carpeta.join(format!("biblioteca.{usuario}.json"))
}

/// Ficheros de biblioteca de la carpeta de `ruta` (`biblioteca*.json`), con
/// `ruta` el primero aunque aún no exista.
pub fn ficheros(ruta: &Path) -> Vec<PathBuf> {
    let mut v = vec![ruta.to_path_buf()];
    let Some(carpeta) = ruta.parent() else { return v };
    let Ok(rd) = std::fs::read_dir(carpeta) else { return v };
    let mut otros: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p != ruta)
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                let n = n.to_ascii_lowercase();
                n.starts_with("biblioteca") && n.ends_with(".json")
            })
        })
        .collect();
    otros.sort();
    v.extend(otros);
    v
}

// ------------------------------------------------------------ claves

const SEP: char = '\u{1f}';

/// Un fabricante como datos sueltos y sus sellos.
type Plano<'a> = (BTreeMap<String, Value>, &'a BTreeMap<String, Sello>);

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Clave de una oferta: su número (o la fecha si no tiene).
fn clave_oferta(o: &Oferta) -> String {
    if o.numero.is_empty() {
        format!("of:~{}", o.fecha)
    } else {
        format!("of:{}", o.numero)
    }
}

/// Un fabricante como datos sueltos: clave → valor.
fn aplanar(f: &Fabricante) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    let grupos: BTreeSet<&String> = f.descuentos.keys().chain(f.cadenas.keys()).collect();
    for g in grupos {
        m.insert(
            format!("dto:{g}"),
            serde_json::json!({ "d": f.descuentos.get(g), "c": f.cadenas.get(g) }),
        );
    }
    for t in &f.tarifas {
        m.insert(
            format!("tar:{}", t.etiqueta),
            serde_json::json!({ "anio": t.anio, "cargada": t.cargada }),
        );
        for (r, l) in &t.lineas {
            m.insert(format!("pvp:{}{SEP}{r}", t.etiqueta), json(l));
        }
    }
    for o in &f.ofertas {
        m.insert(clave_oferta(o), json(o));
    }
    if let Some(s) = f.subida_estimada {
        m.insert("subida".into(), json(&s));
    }
    if f.datos != DatosFabricante::default() {
        m.insert("datos".into(), json(&f.datos));
    }
    m
}

/// Rehace un fabricante con sus datos sueltos.
fn rehacer(nombre: &str, m: &BTreeMap<String, Value>, sellos: BTreeMap<String, Sello>) -> Fabricante {
    let mut f = Fabricante {
        nombre: nombre.to_owned(),
        sellos,
        ..Fabricante::default()
    };
    let mut tarifas: BTreeMap<String, TarifaFabricante> = BTreeMap::new();
    for (k, v) in m {
        if let Some(g) = k.strip_prefix("dto:") {
            if let Some(d) = v
                .get("d")
                .and_then(|d| serde_json::from_value::<Decimal>(d.clone()).ok())
            {
                f.descuentos.insert(g.to_owned(), d);
            }
            if let Some(c) = v
                .get("c")
                .and_then(|c| serde_json::from_value::<Vec<Decimal>>(c.clone()).ok())
            {
                f.cadenas.insert(g.to_owned(), c);
            }
        } else if let Some(e) = k.strip_prefix("tar:") {
            tarifas.insert(
                e.to_owned(),
                TarifaFabricante {
                    etiqueta: e.to_owned(),
                    anio: v
                        .get("anio")
                        .and_then(Value::as_i64)
                        .and_then(|a| i32::try_from(a).ok())
                        .unwrap_or(0),
                    cargada: v.get("cargada").and_then(Value::as_str).unwrap_or_default().to_owned(),
                    lineas: BTreeMap::new(),
                },
            );
        } else if k.starts_with("of:") {
            if let Ok(o) = serde_json::from_value::<Oferta>(v.clone()) {
                f.ofertas.push(o);
            }
        } else if k == "subida" {
            f.subida_estimada = serde_json::from_value(v.clone()).ok();
        } else if k == "datos" {
            f.datos = serde_json::from_value(v.clone()).unwrap_or_default();
        }
    }
    for (k, v) in m {
        let Some((e, r)) = k.strip_prefix("pvp:").and_then(|x| x.split_once(SEP)) else {
            continue;
        };
        // Las líneas de una tarifa borrada se van con ella
        if let (Some(t), Ok(l)) = (tarifas.get_mut(e), serde_json::from_value::<LineaPvp>(v.clone())) {
            t.lineas.insert(r.to_owned(), l);
        }
    }
    f.tarifas = tarifas.into_values().collect();
    f.tarifas
        .sort_by(|a, b| (a.anio, &a.etiqueta).cmp(&(b.anio, &b.etiqueta)));
    f.ofertas.sort_by(|a, b| a.fecha.cmp(&b.fecha));
    f
}

// ------------------------------------------------------------ sellar y juntar

impl Biblioteca {
    /// Pone el sello `s` a todo lo que ha cambiado (o se ha borrado) de
    /// `antes` a `self`.
    pub fn sellar(&mut self, antes: &Biblioteca, s: &Sello) {
        let nombres: BTreeSet<String> = antes
            .fabricantes
            .keys()
            .chain(self.fabricantes.keys())
            .cloned()
            .collect();
        for n in nombres {
            let a = antes.fabricantes.get(&n).map(aplanar).unwrap_or_default();
            let Some(f) = self.fabricantes.get_mut(&n) else {
                continue;
            };
            let d = aplanar(f);
            let claves: BTreeSet<&String> = a.keys().chain(d.keys()).collect();
            for k in claves {
                if a.get(k) != d.get(k) {
                    f.sellos.insert(k.clone(), s.clone());
                }
            }
        }
        let claves: BTreeSet<String> = antes
            .equivalencias
            .keys()
            .chain(self.equivalencias.keys())
            .cloned()
            .collect();
        for k in claves {
            if antes.equivalencias.get(&k) != self.equivalencias.get(&k) {
                self.sellos.insert(format!("eq:{k}"), s.clone());
            }
        }
        let claves: BTreeSet<String> = antes.kits.keys().chain(self.kits.keys()).cloned().collect();
        for k in claves {
            if antes.kits.get(&k) != self.kits.get(&k) {
                self.sellos.insert(format!("kit:{k}"), s.clone());
            }
        }
    }

    /// Junta varias bibliotecas (la primera, la propia): por cada dato, el
    /// del sello más reciente; los datos sin sello (de antes de compartir)
    /// cuentan como los más antiguos y, entre ellos, gana la primera fuente
    /// que los tiene. Las versiones de partidas se suman sin repetir.
    pub fn juntar(fuentes: &[Biblioteca]) -> Biblioteca {
        if fuentes.len() == 1 {
            return fuentes[0].clone();
        }
        let mut b = Biblioteca::default();
        // Partidas
        for f in fuentes {
            for (c, vs) in &f.partidas {
                let dest = b.partidas.entry(c.clone()).or_default();
                for v in vs {
                    if !dest.contains(v) {
                        dest.push(v.clone());
                    }
                }
            }
        }
        for vs in b.partidas.values_mut() {
            vs.sort_by(|a, b| a.fecha.cmp(&b.fecha));
        }
        // Fabricantes
        let nombres: BTreeSet<&String> = fuentes.iter().flat_map(|f| f.fabricantes.keys()).collect();
        for n in nombres {
            let planos: Vec<Option<Plano>> = fuentes
                .iter()
                .map(|f| f.fabricantes.get(n).map(|x| (aplanar(x), &x.sellos)))
                .collect();
            let claves: BTreeSet<String> = planos
                .iter()
                .flatten()
                .flat_map(|(m, s)| m.keys().chain(s.keys()).cloned())
                .collect();
            let mut m = BTreeMap::new();
            let mut sellos = BTreeMap::new();
            for k in claves {
                // Fuentes que saben algo de este dato: lo tienen o lo sellaron (borrado)
                let mejor = planos
                    .iter()
                    .flatten()
                    .filter(|(mm, ss)| mm.contains_key(&k) || ss.contains_key(&k))
                    .fold(None::<(Option<&Sello>, Option<&Value>)>, |acc, (mm, ss)| {
                        let s = ss.get(&k);
                        match acc {
                            Some((sa, _)) if s <= sa => acc,
                            _ => Some((s, mm.get(&k))),
                        }
                    });
                if let Some((s, v)) = mejor {
                    if let Some(s) = s {
                        sellos.insert(k.clone(), s.clone());
                    }
                    if let Some(v) = v {
                        m.insert(k, v.clone());
                    }
                }
            }
            b.fabricantes.insert(n.clone(), rehacer(n, &m, sellos));
        }
        // Equivalencias
        let claves: BTreeSet<&str> = fuentes
            .iter()
            .flat_map(|f| {
                f.equivalencias
                    .keys()
                    .map(String::as_str)
                    .chain(f.sellos.keys().filter_map(|k| k.strip_prefix("eq:")))
            })
            .collect();
        for clave in claves {
            let sk = format!("eq:{clave}");
            let mejor = fuentes
                .iter()
                .filter(|f| f.equivalencias.contains_key(clave) || f.sellos.contains_key(&sk))
                .fold(None::<(Option<&Sello>, Option<&crate::Equivalencia>)>, |acc, f| {
                    let s = f.sellos.get(&sk);
                    match acc {
                        Some((sa, _)) if s <= sa => acc,
                        _ => Some((s, f.equivalencias.get(clave))),
                    }
                });
            if let Some((s, e)) = mejor {
                if let Some(s) = s {
                    b.sellos.insert(sk, s.clone());
                }
                if let Some(e) = e {
                    b.equivalencias.insert(clave.to_owned(), e.clone());
                }
            }
        }
        // Kits
        let claves: BTreeSet<&str> = fuentes
            .iter()
            .flat_map(|f| {
                f.kits
                    .keys()
                    .map(String::as_str)
                    .chain(f.sellos.keys().filter_map(|k| k.strip_prefix("kit:")))
            })
            .collect();
        for nombre in claves {
            let sk = format!("kit:{nombre}");
            let mejor = fuentes
                .iter()
                .filter(|f| f.kits.contains_key(nombre) || f.sellos.contains_key(&sk))
                .fold(None::<(Option<&Sello>, Option<&crate::Kit>)>, |acc, f| {
                    let s = f.sellos.get(&sk);
                    match acc {
                        Some((sa, _)) if s <= sa => acc,
                        _ => Some((s, f.kits.get(nombre))),
                    }
                });
            if let Some((s, k)) = mejor {
                if let Some(s) = s {
                    b.sellos.insert(sk, s.clone());
                }
                if let Some(k) = k {
                    b.kits.insert(nombre.to_owned(), k.clone());
                }
            }
        }
        b
    }

    /// Último cambio (sello) de un fabricante: quién y cuándo.
    pub fn ultimo_cambio(&self, fabricante: &str) -> Option<&Sello> {
        self.fabricante(fabricante).and_then(|f| f.sellos.values().max())
    }
}

// ------------------------------------------------------------ obra ↔ biblioteca

/// Un precio de la obra que no coincide con la biblioteca.
#[derive(Debug, Clone, PartialEq)]
pub struct Diferencia {
    /// «FABRICANTE ref. X» o «FABRICANTE/GRUPO».
    pub que: String,
    /// Código del recurso en la obra (vacío en descuentos).
    pub codigo: String,
    pub fabricante: String,
    /// Referencia (PVP) o clave del descuento.
    pub clave: String,
    pub obra: Decimal,
    /// `None`: la biblioteca no lo tiene.
    pub biblioteca: Option<Decimal>,
    /// Escalones del descuento en la biblioteca («40+8»).
    pub cadena: Vec<Decimal>,
    /// Último cambio del dato en la biblioteca.
    pub sello: Option<Sello>,
    pub es_descuento: bool,
}

impl Biblioteca {
    /// PVP y descuentos de la obra que no coinciden con la biblioteca: de
    /// cada recurso con tarifa y referencia, el PVP vigente de la biblioteca
    /// (sin subida estimada) y, de cada descuento de la obra, el del mismo
    /// grupo en la biblioteca si lo tiene.
    pub fn diferencias_con(&self, p: &ppto_core::Presupuesto) -> Vec<Diferencia> {
        let mut v = Vec::new();
        for c in p.conceptos.values() {
            let (Some(t), Some(r)) = (c.tarifa.as_ref(), ppto_core::Presupuesto::referencia_tarifa(c)) else {
                continue;
            };
            let fab = self.fabricante(&t.fabricante);
            let tar = fab.and_then(|f| f.tarifas.iter().rev().find(|x| x.lineas.contains_key(&r)));
            let pvp = tar.map(|x| x.lineas[&r].pvp);
            if pvp != Some(t.pvp) {
                v.push(Diferencia {
                    que: format!("{} ref. {r}", ppto_core::descuentos::normalizar(&t.fabricante)),
                    codigo: c.codigo.clone(),
                    fabricante: ppto_core::descuentos::normalizar(&t.fabricante),
                    clave: r.clone(),
                    obra: t.pvp,
                    biblioteca: pvp,
                    cadena: Vec::new(),
                    sello: fab
                        .zip(tar)
                        .and_then(|(f, x)| f.sellos.get(&format!("pvp:{}{SEP}{r}", x.etiqueta)).cloned()),
                    es_descuento: false,
                });
            }
        }
        for (k, d) in &p.descuentos {
            let (f, g) = ppto_core::descuentos::partes(k);
            let fab = self.fabricante(&f);
            let g = g.unwrap_or_default();
            let db = fab.and_then(|x| x.descuentos.get(&g)).copied();
            if db.map(|x| x.normalize()) != Some(d.normalize()) {
                v.push(Diferencia {
                    que: format!("descuento {k}"),
                    codigo: String::new(),
                    fabricante: f.clone(),
                    clave: k.clone(),
                    obra: *d,
                    biblioteca: db,
                    cadena: fab
                        .and_then(|x| x.cadenas.get(&g).cloned())
                        .or_else(|| db.map(|x| vec![x.normalize()]))
                        .unwrap_or_default(),
                    sello: fab.and_then(|x| x.sellos.get(&format!("dto:{g}")).cloned()),
                    es_descuento: true,
                });
            }
        }
        v
    }

    /// Pasa a la biblioteca los PVP y descuentos de la obra (los que
    /// cambian; los PVP en la tarifa del año `anio`, que se crea si hace
    /// falta, sin tocar el resto de referencias) y guarda sus partidas.
    /// Devuelve cuántos PVP y descuentos han cambiado.
    pub fn actualizar_con(&mut self, p: &ppto_core::Presupuesto, obra: &str, anio: i32) -> (usize, usize) {
        let (mut n_pvp, mut n_dto) = (0, 0);
        for c in p.conceptos.values() {
            let (Some(t), Some(r)) = (c.tarifa.as_ref(), ppto_core::Presupuesto::referencia_tarifa(c)) else {
                continue;
            };
            let actual = self
                .fabricante(&t.fabricante)
                .and_then(|f| f.tarifas.iter().rev().find(|x| x.lineas.contains_key(&r)))
                .map(|x| x.lineas[&r].pvp);
            if actual == Some(t.pvp) {
                continue;
            }
            let fab = self.fabricante_mut(&t.fabricante);
            let et = anio.to_string();
            if !fab.tarifas.iter().any(|x| x.etiqueta == et) {
                fab.tarifas.push(TarifaFabricante {
                    etiqueta: et.clone(),
                    anio,
                    cargada: crate::hoy(),
                    lineas: BTreeMap::new(),
                });
                fab.tarifas
                    .sort_by(|a, b| (a.anio, &a.etiqueta).cmp(&(b.anio, &b.etiqueta)));
            }
            if let Some(tar) = fab.tarifas.iter_mut().find(|x| x.etiqueta == et) {
                tar.lineas.insert(
                    r,
                    LineaPvp {
                        descripcion: c.resumen.clone(),
                        unidad: c.unidad.clone(),
                        pvp: t.pvp,
                        grupo: t.grupo.clone(),
                    },
                );
                n_pvp += 1;
            }
        }
        for k in p.descuentos.keys() {
            let (f, g) = ppto_core::descuentos::partes(k);
            let g = g.unwrap_or_default();
            let cadena = p.cadena_descuento(k);
            let igual = self.fabricante(&f).is_some_and(|x| {
                x.descuentos.get(&g).map(|d| d.normalize()) == p.descuentos.get(k).map(|d| d.normalize())
            });
            if !igual {
                self.fijar_descuento_encadenado(k, Some(&cadena));
                n_dto += 1;
            }
        }
        self.guardar_obra(p, obra, crate::Origen::Revisado, None);
        (n_pvp, n_dto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn sello(cuando: &str, quien: &str) -> Sello {
        Sello {
            cuando: cuando.into(),
            quien: quien.into(),
        }
    }

    fn lp(pvp: Decimal) -> LineaPvp {
        LineaPvp {
            descripcion: "Artículo".into(),
            unidad: "ud".into(),
            pvp,
            grupo: None,
        }
    }

    /// Dos personas cambian a la vez cosas distintas del mismo fabricante y
    /// la misma cosa: lo distinto se suma y en lo mismo gana lo último.
    #[test]
    fn dos_personas_cambian_a_la_vez() {
        let mut base = Biblioteca::default();
        base.fijar_descuento("FABA", Some(dec!(40)));
        base.cargar_tarifa(
            "FABA",
            "2026",
            2026,
            BTreeMap::from([("R1".into(), lp(dec!(100))), ("R2".into(), lp(dec!(50)))]),
        );

        // A: sube R1 y pone descuento 45
        let mut a = base.clone();
        a.cargar_tarifa(
            "FABA",
            "2026",
            2026,
            BTreeMap::from([("R1".into(), lp(dec!(110))), ("R2".into(), lp(dec!(50)))]),
        );
        a.fijar_descuento("FABA", Some(dec!(45)));
        a.sellar(&base, &sello("2026-11-12 10:00:00", "ana"));
        // B (un poco después): sube R2, descuento 50 y oferta nueva
        let mut b = base.clone();
        b.cargar_tarifa(
            "FABA",
            "2026",
            2026,
            BTreeMap::from([("R1".into(), lp(dec!(100))), ("R2".into(), lp(dec!(55)))]),
        );
        b.fijar_descuento("FABA", Some(dec!(50)));
        b.anadir_oferta(
            "FABA",
            Oferta {
                numero: "PE1".into(),
                fecha: "2026-11-12".into(),
                validez: None,
                obra: "Obra sintética".into(),
                lineas: vec![],
                documento: None,
            },
        );
        b.sellar(&base, &sello("2026-11-12 10:05:00", "beto"));

        for orden in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
            let j = Biblioteca::juntar(&orden);
            let f = j.fabricante("FABA").unwrap();
            assert_eq!(j.pvp("FABA", "R1", 2026).unwrap().pvp, dec!(110), "R1 de ana");
            assert_eq!(j.pvp("FABA", "R2", 2026).unwrap().pvp, dec!(55), "R2 de beto");
            assert_eq!(f.descuento(None), dec!(50), "descuento: el más reciente");
            assert_eq!(f.ofertas.len(), 1);
            assert_eq!(j.ultimo_cambio("FABA").unwrap().quien, "beto");
        }
    }

    /// Un borrado con sello más reciente gana a un dato antiguo de otro
    /// fichero; un dato sin sello (de antes de compartir) se conserva.
    #[test]
    fn borrados_y_datos_antiguos() {
        let mut viejo = Biblioteca::default();
        viejo.fijar_descuento("FABB", Some(dec!(30)));
        viejo.fijar_subida("FABB", Some(dec!(3)));
        let mut mio = viejo.clone();
        mio.fijar_subida("FABB", None);
        mio.sellar(&viejo, &sello("2026-11-12 11:00:00", "ana"));
        let j = Biblioteca::juntar(&[mio, viejo]);
        let f = j.fabricante("FABB").unwrap();
        assert_eq!(f.subida_estimada, None, "el borrado sellado gana");
        assert_eq!(f.descuento(None), dec!(30), "el dato sin sello se conserva");
    }

    #[test]
    fn ficheros_de_la_carpeta() {
        let dir = std::env::temp_dir().join(format!("bib-compartida-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in [
            "biblioteca.json",
            "biblioteca.beto.json",
            "otra.json",
            "biblioteca.ana.tmp1",
        ] {
            std::fs::write(dir.join(n), "{}").unwrap();
        }
        let propio = fichero_de(&dir, "ana");
        let v = ficheros(&propio);
        let nombres: Vec<String> = v
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            nombres,
            ["biblioteca.ana.json", "biblioteca.beto.json", "biblioteca.json"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hora_y_usuario() {
        let a = ahora();
        assert_eq!(a.len(), 23, "{a}");
        assert!(
            usuario()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        );
    }

    /// Obra ↔ biblioteca: diferencias de PVP y descuento, pasar la obra a la
    /// biblioteca sin perder el resto de la tarifa.
    #[test]
    fn obra_y_biblioteca() {
        use ppto_core::concepto::Naturaleza;
        let mut b = Biblioteca::default();
        b.cargar_tarifa(
            "FABC",
            "2026",
            2026,
            BTreeMap::from([("R1".into(), lp(dec!(100))), ("R2".into(), lp(dec!(80)))]),
        );
        b.fijar_descuento("FABC", Some(dec!(40)));
        let mut p = ppto_core::Presupuesto::new("Obra sintética", "OBRA", "Obra");
        p.nuevo_recurso("M.R1", "ud", "Artículo 1", Naturaleza::Material, dec!(0))
            .unwrap();
        p.fijar_tarifa_grupo("M.R1", "FABC", None, dec!(120)).unwrap();
        p.fijar_referencia_tarifa("M.R1", Some("R1")).unwrap();
        p.fijar_descuento("FABC", dec!(45)).unwrap();
        let d = b.diferencias_con(&p);
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(
            d.iter()
                .any(|x| !x.es_descuento && x.obra == dec!(120) && x.biblioteca == Some(dec!(100)))
        );
        assert!(
            d.iter()
                .any(|x| x.es_descuento && x.obra == dec!(45) && x.biblioteca == Some(dec!(40)))
        );
        let (n_pvp, n_dto) = b.actualizar_con(&p, "Obra sintética", 2026);
        assert_eq!((n_pvp, n_dto), (1, 1));
        assert!(b.diferencias_con(&p).is_empty());
        assert_eq!(
            b.pvp("FABC", "R2", 2026).unwrap().pvp,
            dec!(80),
            "el resto de la tarifa sigue"
        );
    }
}

#[cfg(test)]
mod pruebas_kits {
    use super::*;
    use crate::Kit;

    fn kit(nombre: &str) -> Kit {
        let p = ppto_core::ejemplos::suelo_radiante();
        let l: Vec<String> = p.conceptos["SR.M2"]
            .descomposicion
            .iter()
            .take(2)
            .map(|l| l.hijo.clone())
            .collect();
        Kit {
            nombre: nombre.into(),
            unidad: "ud".into(),
            lineas: ppto_core::fragmento::copiar(&p, "SR.M2", &l).unwrap(),
        }
    }

    #[test]
    fn kits_se_comparten_y_se_borran() {
        let s1 = Sello {
            cuando: "2026-11-12 10:00:00.000".into(),
            quien: "a".into(),
        };
        let s2 = Sello {
            cuando: "2026-11-12 11:00:00.000".into(),
            quien: "b".into(),
        };
        let mut a = Biblioteca::default();
        let antes = a.clone();
        a.kits.insert("Toma fancoil".into(), kit("Toma fancoil"));
        a.sellar(&antes, &s1);
        assert!(a.kits["Toma fancoil"].precio() > Decimal::ZERO);
        // El compañero lo recibe y lo borra más tarde: gana el borrado
        let mut b = Biblioteca::juntar(&[Biblioteca::default(), a.clone()]);
        assert!(b.kits.contains_key("Toma fancoil"));
        let antes = b.clone();
        b.kits.clear();
        b.sellar(&antes, &s2);
        let j = Biblioteca::juntar(&[a, b]);
        assert!(j.kits.is_empty());
        assert_eq!(j.sellos["kit:Toma fancoil"].quien, "b");
    }
}
