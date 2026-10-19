// SPDX-License-Identifier: GPL-3.0-or-later
//! Códigos cortos para programas antiguos.
//!
//! Algunas versiones antiguas de Presto cortan los códigos de concepto a unos
//! pocos caracteres al importar un BC3. Si dos códigos comparten el principio,
//! al cortarlos se convierten en el mismo concepto y el presupuesto cambia de
//! precio sin avisar (p. ej. `UMAD22DMT20001` y `UMAD22DMT20005`, o un recurso
//! y su línea de descuento `<código>%`). Aquí se renombran antes de exportar.

use crate::Presupuesto;
use std::collections::{BTreeMap, BTreeSet};

impl Presupuesto {
    /// Copia del presupuesto en la que ningún código pasa de `max` caracteres.
    ///
    /// Los códigos más largos se renombran a un código único de `max − 1`
    /// caracteres como mucho (se deja sitio para la línea de descuento
    /// `<código>%`): se corta el código y, si ya existe, se cambia el final
    /// por un número. El cambio es el mismo en conceptos, descompuestos y
    /// mediciones. Devuelve un aviso por código renombrado.
    pub fn con_codigos_cortos(&self, max: usize) -> (Presupuesto, Vec<String>) {
        let max = max.max(4);
        let largos: Vec<&String> = self.conceptos.keys().filter(|c| c.chars().count() > max).collect();
        if largos.is_empty() {
            return (self.clone(), Vec::new());
        }
        let mut usados: BTreeSet<String> = self
            .conceptos
            .keys()
            .filter(|c| c.chars().count() <= max)
            .cloned()
            .collect();
        let mut mapa: BTreeMap<String, String> = BTreeMap::new();
        let mut avisos = Vec::new();
        let n = max - 1;
        // Primero los códigos normales y después los porcentuales, que llevan
        // delante la máscara (`<código>%`): la máscara sigue a su recurso.
        let (porcentuales, normales): (Vec<&String>, Vec<&String>) = largos.into_iter().partition(|c| c.contains('%'));
        for codigo in normales {
            let base: String = codigo.chars().take(n).collect();
            let nuevo = libre(&base, n, &usados);
            usados.insert(nuevo.clone());
            avisos.push(format!("código «{codigo}» exportado como «{nuevo}»"));
            mapa.insert(codigo.clone(), nuevo);
        }
        for codigo in porcentuales {
            let (mascara, resto) = codigo.split_once('%').unwrap_or((codigo, ""));
            let mascara = mapa.get(mascara).map_or(mascara, String::as_str);
            let mut nuevo: String = format!("{mascara}%{resto}").chars().take(max).collect();
            let mut i = 1u32;
            while usados.contains(&nuevo) || !nuevo.contains('%') {
                let sufijo = i.to_string();
                let cabe = max.saturating_sub(mascara.chars().count() + 1 + sufijo.len());
                nuevo = format!("{mascara}%{}{sufijo}", resto.chars().take(cabe).collect::<String>());
                if nuevo.chars().count() > max {
                    // La máscara no deja sitio: el porcentual se aplica a todo
                    nuevo = libre(&format!("%{resto}"), max, &usados);
                    break;
                }
                i += 1;
            }
            usados.insert(nuevo.clone());
            avisos.push(format!("código «{codigo}» exportado como «{nuevo}»"));
            mapa.insert(codigo.clone(), nuevo);
        }
        let cambia = |c: &String| mapa.get(c).cloned().unwrap_or_else(|| c.clone());
        let mut p = self.clone();
        p.raiz = cambia(&self.raiz);
        p.conceptos = self
            .conceptos
            .iter()
            .map(|(k, c)| {
                let mut c = c.clone();
                c.codigo = cambia(k);
                for l in &mut c.descomposicion {
                    l.hijo = cambia(&l.hijo);
                }
                (c.codigo.clone(), c)
            })
            .collect();
        p.mediciones = self
            .mediciones
            .iter()
            .map(|((a, b), m)| ((cambia(a), cambia(b)), m.clone()))
            .collect();
        (p, avisos)
    }
}

/// Primer código libre a partir de `base` (cortado a `n`), cambiando el
/// final por un número si ya existe. No usa uno cuya línea de descuento
/// `<código>%` ya exista.
fn libre(base: &str, n: usize, usados: &BTreeSet<String>) -> String {
    let base: String = base.chars().take(n).collect();
    let ocupado = |c: &str| usados.contains(c) || usados.contains(&format!("{c}%"));
    let mut nuevo = base.clone();
    let mut i = 1u32;
    while ocupado(&nuevo) {
        let sufijo = i.to_string();
        let corte = n.saturating_sub(sufijo.len());
        nuevo = format!("{}{sufijo}", base.chars().take(corte).collect::<String>());
        i += 1;
    }
    nuevo
}
