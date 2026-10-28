// SPDX-License-Identifier: GPL-3.0-or-later
//! Copiar y pegar partidas o capítulos **entre obras** (Ctrl+C en una
//! ventana, Ctrl+V en otra), como en Presto.
//!
//! Lo copiado viaja por el portapapeles del sistema como texto
//! (`PPTO-BC3/1` + JSON): la partida o el capítulo con todo lo que cuelga de
//! ellos (descompuestos, recursos, mediciones) y los descuentos de los
//! fabricantes que usan.
//!
//! Al pegar en otra obra, para no estropear nada de lo que ya hay:
//! - Lo que ya existe **igual** se reutiliza.
//! - Una **partida o capítulo** con el mismo código pero distinto contenido
//!   entra con un código nuevo (`CÓDIGO_1`…), para que el precio pegado sea
//!   el de la obra de origen.
//! - Un **recurso** (mano de obra, material…) con el mismo código y otro
//!   precio usa el de la obra de destino: sus precios mandan.
//! - Los descuentos de fabricante solo se copian si la obra no los tiene.

use crate::concepto::{Concepto, Naturaleza};
use crate::error::ErrorMotor;
use crate::medicion::Medicion;
use crate::presupuesto::Presupuesto;
use crate::{Decimal, LineaDescomposicion};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CABECERA: &str = "PPTO-BC3/1";

/// Una línea pegada en el capítulo de destino.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaFragmento {
    pub linea: LineaDescomposicion,
    pub medicion: Option<Medicion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fragmento {
    /// Obra de origen (para los mensajes).
    pub obra: String,
    pub conceptos: Vec<Concepto>,
    pub lineas: Vec<LineaFragmento>,
    /// Mediciones internas de los capítulos copiados: (padre, hijo, medición).
    pub mediciones: Vec<(String, String, Medicion)>,
    pub descuentos: BTreeMap<String, Decimal>,
}

/// Resultado de pegar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InformePegado {
    /// Códigos pegados en el capítulo de destino (ya renombrados).
    pub pegados: Vec<String>,
    /// Conceptos nuevos en la obra.
    pub nuevos: usize,
    /// (código en origen, código nuevo) de partidas o capítulos renombrados.
    pub renombrados: Vec<(String, String)>,
    /// Recursos con el mismo código y otro precio: se usa el de esta obra.
    pub recursos_de_esta_obra: Vec<String>,
}

impl Fragmento {
    /// Texto para el portapapeles.
    pub fn a_texto(&self) -> String {
        format!("{CABECERA}\n{}", serde_json::to_string(self).unwrap_or_default())
    }

    /// Lee un texto del portapapeles (`None` si no es un fragmento).
    pub fn de_texto(t: &str) -> Option<Self> {
        let resto = t.trim_start().strip_prefix(CABECERA)?;
        serde_json::from_str(resto.trim()).ok()
    }
}

/// Copia `hijos` (partidas o capítulos) del capítulo `padre`.
pub fn copiar(p: &Presupuesto, padre: &str, hijos: &[String]) -> Result<Fragmento, ErrorMotor> {
    let cap = p.concepto(padre)?;
    let mut lineas = Vec::new();
    let mut pendientes = Vec::new();
    for h in hijos {
        let l = cap
            .descomposicion
            .iter()
            .find(|l| &l.hijo == h)
            .ok_or_else(|| ErrorMotor::RecursoNoEnPartida {
                partida: padre.to_owned(),
                recurso: h.clone(),
            })?;
        lineas.push(LineaFragmento {
            linea: l.clone(),
            medicion: p.mediciones.get(&(padre.to_owned(), h.clone())).cloned(),
        });
        pendientes.push(h.clone());
    }
    let mut vistos = BTreeSet::new();
    let mut conceptos = Vec::new();
    let mut mediciones = Vec::new();
    while let Some(c) = pendientes.pop() {
        if !vistos.insert(c.clone()) {
            continue;
        }
        let con = p.concepto(&c)?;
        if con.naturaleza == Naturaleza::Capitulo {
            for l in &con.descomposicion {
                if let Some(m) = p.mediciones.get(&(c.clone(), l.hijo.clone())) {
                    mediciones.push((c.clone(), l.hijo.clone(), m.clone()));
                }
            }
        }
        pendientes.extend(con.descomposicion.iter().map(|l| l.hijo.clone()));
        conceptos.push(con.clone());
    }
    let mut descuentos = BTreeMap::new();
    for c in &conceptos {
        if let Some(t) = &c.tarifa {
            let k = p.clave_aplicada(t);
            if let Some(d) = p.descuentos.get(&k) {
                descuentos.insert(k, *d);
            }
        }
    }
    Ok(Fragmento {
        obra: p.conceptos.get(&p.raiz).map(|c| c.resumen.clone()).unwrap_or_default(),
        conceptos,
        lineas,
        mediciones,
        descuentos,
    })
}

/// Pega el fragmento al final del capítulo `destino`. Todo o nada.
pub fn pegar(p: &mut Presupuesto, f: &Fragmento, destino: &str) -> Result<InformePegado, ErrorMotor> {
    let copia = p.clone();
    let r = pegar_sin_deshacer(p, f, destino);
    if r.is_err() {
        *p = copia;
    }
    r
}

fn pegar_sin_deshacer(p: &mut Presupuesto, f: &Fragmento, destino: &str) -> Result<InformePegado, ErrorMotor> {
    if p.concepto(destino)?.naturaleza != Naturaleza::Capitulo {
        return Err(ErrorMotor::ValorInvalido(format!("«{destino}» no es un capítulo")));
    }
    let mut inf = InformePegado::default();
    // 1. Qué entra, qué se reutiliza y qué se renombra
    let mut nombre: BTreeMap<String, String> = BTreeMap::new();
    let mut usados: BTreeSet<String> = p.conceptos.keys().cloned().collect();
    let mut insertar = Vec::new();
    for c in &f.conceptos {
        match p.conceptos.get(&c.codigo) {
            None => {
                nombre.insert(c.codigo.clone(), c.codigo.clone());
                insertar.push(c.clone());
            }
            Some(ya) if ya == c => {
                nombre.insert(c.codigo.clone(), c.codigo.clone());
            }
            Some(_) if c.naturaleza.es_basico() || c.naturaleza == Naturaleza::Porcentaje => {
                // Recurso con otro precio: manda el de esta obra
                nombre.insert(c.codigo.clone(), c.codigo.clone());
                inf.recursos_de_esta_obra.push(c.codigo.clone());
            }
            Some(_) => {
                let nuevo = (1..)
                    .map(|n| format!("{}_{n}", c.codigo))
                    .find(|x| !usados.contains(x))
                    .unwrap_or_default();
                usados.insert(nuevo.clone());
                inf.renombrados.push((c.codigo.clone(), nuevo.clone()));
                nombre.insert(c.codigo.clone(), nuevo);
                insertar.push(c.clone());
            }
        }
    }
    let n = |c: &str| nombre.get(c).cloned().unwrap_or_else(|| c.to_owned());
    // 2. Conceptos nuevos, con las referencias renombradas
    for mut c in insertar {
        c.codigo = n(&c.codigo);
        for l in &mut c.descomposicion {
            l.hijo = n(&l.hijo);
        }
        p.conceptos.insert(c.codigo.clone(), c);
        inf.nuevos += 1;
    }
    for (padre, hijo, m) in &f.mediciones {
        p.mediciones.entry((n(padre), n(hijo))).or_insert_with(|| m.clone());
    }
    for (k, d) in &f.descuentos {
        p.descuentos.entry(k.clone()).or_insert(*d);
    }
    // 3. Líneas en el capítulo de destino
    for l in &f.lineas {
        let hijo = n(&l.linea.hijo);
        p.anadir_linea(destino, &hijo, l.linea.rendimiento)?;
        if let Some(x) = p.concepto_mut(destino)?.descomposicion.last_mut() {
            x.factor = l.linea.factor;
        }
        if let Some(m) = &l.medicion {
            p.mediciones.insert((destino.to_owned(), hijo.clone()), m.clone());
        }
        inf.pegados.push(hijo);
    }
    p.recalcular_tarifas();
    p.validar()?;
    Ok(inf)
}
