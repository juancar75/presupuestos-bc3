// SPDX-License-Identifier: GPL-3.0-or-later
//! Valorar a mano más rápido: buscador único al añadir una línea (obra,
//! biblioteca de precios y tarifas de subcontrata) y copiar el descompuesto
//! de la partida anterior.

use super::*;
use ppto_biblioteca::Biblioteca;

/// Un resultado del buscador.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Hallazgo {
    /// Concepto que ya está en la obra.
    Obra(String),
    /// Artículo de una tarifa de fabricante de la biblioteca.
    Tarifa(Box<ArticuloTarifa>),
    /// Línea de una tarifa de subcontrata.
    Subcontrata(ppto_tarifas::Encontrada),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ArticuloTarifa {
    pub fabricante: String,
    pub referencia: String,
    pub descripcion: String,
    pub unidad: String,
    pub pvp: Decimal,
    pub grupo: Option<String>,
    /// Clave y escalones del descuento de la biblioteca.
    pub descuento: Option<(String, Vec<Decimal>)>,
    /// Etiqueta de la tarifa («2026»).
    pub tarifa: String,
}

impl ArticuloTarifa {
    pub fn neto(&self) -> Decimal {
        let mut n = self.pvp;
        for d in self.descuento.iter().flat_map(|(_, c)| c) {
            n = n * (Decimal::ONE_HUNDRED - d) / Decimal::ONE_HUNDRED;
        }
        n.round_dp(2)
    }
}

/// Texto sin mayúsculas ni tildes.
pub(super) fn plano(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

/// ¿Están todas las palabras de `q` en `texto`?
fn encaja(q: &[String], texto: &str) -> bool {
    let t = plano(texto);
    q.iter().all(|p| t.contains(p.as_str()))
}

/// Artículos de las tarifas de la biblioteca (la más reciente de cada
/// referencia) que contienen todas las palabras.
pub(super) fn buscar_en_biblioteca(b: &Biblioteca, q: &str, max: usize) -> Vec<ArticuloTarifa> {
    let palabras: Vec<String> = plano(q).split_whitespace().map(str::to_owned).collect();
    if palabras.is_empty() {
        return Vec::new();
    }
    let mut v = Vec::new();
    for f in b.fabricantes.values() {
        let mut vistas = std::collections::BTreeSet::new();
        for t in f.tarifas.iter().rev() {
            for (r, l) in &t.lineas {
                if !vistas.insert(r.clone()) {
                    continue;
                }
                if !encaja(
                    &palabras,
                    &format!(
                        "{} {r} {} {}",
                        f.nombre,
                        l.descripcion,
                        l.grupo.as_deref().unwrap_or("")
                    ),
                ) {
                    continue;
                }
                let g = l.grupo.clone().unwrap_or_default();
                let clave = if g.is_empty() {
                    f.nombre.clone()
                } else {
                    format!("{}/{g}", f.nombre)
                };
                let descuento = f
                    .descuentos
                    .get(&g)
                    .map(|d| (clave, f.cadenas.get(&g).cloned().unwrap_or_else(|| vec![d.normalize()])));
                v.push(ArticuloTarifa {
                    fabricante: f.nombre.clone(),
                    referencia: r.clone(),
                    descripcion: l.descripcion.clone(),
                    unidad: l.unidad.clone(),
                    pvp: l.pvp,
                    grupo: l.grupo.clone(),
                    descuento,
                    tarifa: t.etiqueta.clone(),
                });
                if v.len() >= max {
                    return v;
                }
            }
        }
    }
    v
}

impl Aplicacion {
    /// Resultados del buscador para añadir una línea a `padre`.
    pub(super) fn buscar_todo(
        &self,
        q: &str,
        padre: &str,
        bib: Option<&Biblioteca>,
        solo_partidas: bool,
    ) -> Vec<(Hallazgo, String, String)> {
        let palabras: Vec<String> = plano(q).split_whitespace().map(str::to_owned).collect();
        let usados: Vec<&str> = self.p.conceptos[padre]
            .descomposicion
            .iter()
            .map(|l| l.hijo.as_str())
            .collect();
        let mut v = Vec::new();
        // 1. La obra
        let mut refs_obra = std::collections::BTreeSet::new();
        for c in self.p.conceptos.values() {
            if let (Some(t), Some(r)) = (&c.tarifa, Presupuesto::referencia_tarifa(c)) {
                refs_obra.insert((ppto_core::descuentos::normalizar(&t.fabricante), r));
            }
            if c.naturaleza == Naturaleza::Capitulo
                || c.codigo == padre
                || (solo_partidas && c.naturaleza != Naturaleza::Partida)
                || usados.contains(&c.codigo.as_str())
                || !encaja(&palabras, &format!("{} {}", c.codigo, c.resumen))
            {
                continue;
            }
            let precio = self.calc.precios.get(&c.codigo).copied().unwrap_or(c.precio);
            v.push((
                Hallazgo::Obra(c.codigo.clone()),
                format!("{}  ·  {}  ·  {}", c.codigo, c.unidad, corto(&c.resumen, 55)),
                format!("{} € · en la obra", eur(precio)),
            ));
            if v.len() >= 120 {
                break;
            }
        }
        if solo_partidas || palabras.is_empty() {
            return v;
        }
        // 2. Tarifas de fabricante de la biblioteca (lo que la obra no tiene ya)
        if let Some(b) = bib {
            for a in buscar_en_biblioteca(b, q, 80) {
                if refs_obra.contains(&(a.fabricante.clone(), a.referencia.clone())) {
                    continue;
                }
                let dto = a.descuento.as_ref().map_or("sin descuento".into(), |(_, c)| {
                    format!("− {} %", ppto_core::descuentos::texto_cadena(c).replace('.', ","))
                });
                v.push((
                    Hallazgo::Tarifa(Box::new(a.clone())),
                    format!(
                        "{} {}  ·  {}  ·  {}",
                        a.fabricante,
                        a.referencia,
                        a.unidad,
                        corto(&a.descripcion, 45)
                    ),
                    format!("{} {dto} = {} € · tarifa {}", eur(a.pvp), eur(a.neto()), a.tarifa),
                ));
            }
        }
        // 3. Tarifas de subcontrata
        for e in self.tarifas.buscar(q).into_iter().take(40) {
            if let Some((t, l)) = self.tarifas.linea(e) {
                v.push((
                    Hallazgo::Subcontrata(e),
                    format!("{}  ·  {}  ·  {}", t.codigo(l), l.unidad, corto(&l.descripcion, 50)),
                    format!("{} € · subcontrata {}", eur(l.precio), t.nombre()),
                ));
            }
        }
        v
    }

    /// Acción para añadir lo encontrado como línea de `padre`.
    pub(super) fn accion_hallazgo(&self, h: &Hallazgo, padre: &str, q: Decimal) -> Option<Accion> {
        match h {
            Hallazgo::Obra(c) => Some(Accion::AnadirLinea(padre.to_owned(), c.clone(), q)),
            Hallazgo::Subcontrata(e) => self.accion_tarifa(padre, *e, q),
            Hallazgo::Tarifa(a) => {
                let base: String = a
                    .referencia
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                    .collect();
                let mut codigo = format!("M.{}", base.to_uppercase());
                if base.is_empty() || self.p.conceptos.contains_key(&codigo) {
                    codigo = self.p.codigo_libre("M.T", 4);
                }
                // El descuento de la biblioteca solo si la obra no tiene ya uno
                let descuento = a
                    .descuento
                    .clone()
                    .filter(|(k, _)| !self.p.descuentos.contains_key(&ppto_core::descuentos::normalizar(k)));
                Some(Accion::NuevoRecursoTarifa(Box::new(RecursoTarifa {
                    codigo,
                    unidad: a.unidad.clone(),
                    resumen: a.descripcion.clone(),
                    fabricante: a.fabricante.clone(),
                    grupo: a.grupo.clone(),
                    referencia: a.referencia.clone(),
                    pvp: a.pvp,
                    descuento,
                    en: padre.to_owned(),
                    cantidad: q,
                })))
            }
        }
    }

    pub(super) fn nuevo_recurso_tarifa(&mut self, r: &RecursoTarifa) -> Result<(), ppto_core::ErrorMotor> {
        let antes = self.p.clone();
        let res = (|| {
            self.p
                .nuevo_recurso(&r.codigo, &r.unidad, &r.resumen, Naturaleza::Material, Decimal::ZERO)?;
            if let Some((k, c)) = &r.descuento {
                self.p.fijar_descuento_encadenado(k, c)?;
            }
            self.p
                .fijar_tarifa_grupo(&r.codigo, &r.fabricante, r.grupo.as_deref(), r.pvp)?;
            self.p.fijar_referencia_tarifa(&r.codigo, Some(&r.referencia))?;
            self.p.anadir_linea(&r.en, &r.codigo, r.cantidad)
        })();
        if res.is_err() {
            self.p = antes;
        }
        res
    }

    /// Ctrl+D: añade a la partida elegida las líneas (con su cantidad) de la
    /// partida anterior que tenga descompuesto; no repite las que ya tiene.
    pub(super) fn copiar_descompuesto_anterior(&mut self) -> Result<(), ppto_core::ErrorMotor> {
        let Some((_, h)) = self.sel.clone() else {
            return Ok(());
        };
        if self
            .p
            .conceptos
            .get(&h)
            .is_none_or(|c| c.naturaleza != Naturaleza::Partida)
        {
            return Ok(());
        }
        let orden = self.orden_partidas();
        let Some(i) = orden.iter().position(|(_, x)| *x == h) else {
            return Ok(());
        };
        let Some(origen) = orden[..i]
            .iter()
            .rev()
            .map(|(_, x)| x)
            .find(|x| **x != h && self.p.conceptos.get(*x).is_some_and(|c| !c.descomposicion.is_empty()))
            .cloned()
        else {
            self.info("No hay ninguna partida anterior con descompuesto.");
            return Ok(());
        };
        let ya: Vec<String> = self.p.conceptos[&h]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect();
        let lineas: Vec<(String, Decimal)> = self.p.conceptos[&origen]
            .descomposicion
            .iter()
            .filter(|l| !ya.contains(&l.hijo) && l.hijo != h)
            .map(|l| (l.hijo.clone(), l.rendimiento))
            .collect();
        for (x, q) in &lineas {
            self.p.anadir_linea(&h, x, *q)?;
        }
        self.info(format!(
            "Copiadas {} líneas de {origen}. Cambie el material o las cantidades que difieran.",
            lineas.len()
        ));
        Ok(())
    }
}
