// SPDX-License-Identifier: GPL-3.0-or-later
//! Edición estructural del presupuesto (P-014): crear capítulos, partidas y
//! recursos, añadir, quitar y ordenar líneas, y borrar conceptos.
//!
//! Todas las operaciones dejan el presupuesto válido o no lo tocan: si un
//! cambio crearía una referencia circular o un hijo inexistente, se deshace y
//! se devuelve el error.

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza};
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;

/// Caracteres que el formato FIEBDC-3 reserva y no pueden ir en un código.
const RESERVADOS: [char; 4] = ['|', '\\', '~', '#'];

/// Comprueba un código nuevo: no vacío, sin espacios en los extremos ni
/// caracteres reservados de FIEBDC-3.
pub fn validar_codigo(codigo: &str) -> Result<(), ErrorMotor> {
    if codigo.trim().is_empty() {
        return Err(ErrorMotor::ValorInvalido("el código no puede estar vacío".into()));
    }
    if codigo.trim() != codigo {
        return Err(ErrorMotor::ValorInvalido(format!(
            "el código «{codigo}» tiene espacios al principio o al final"
        )));
    }
    if let Some(c) = codigo.chars().find(|c| RESERVADOS.contains(c)) {
        return Err(ErrorMotor::ValorInvalido(format!(
            "el código «{codigo}» contiene «{c}», reservado en BC3"
        )));
    }
    Ok(())
}

impl Presupuesto {
    fn insertar_nuevo(&mut self, c: Concepto) -> Result<(), ErrorMotor> {
        validar_codigo(&c.codigo)?;
        self.insertar(c)
    }

    /// Crea un capítulo y lo cuelga al final de `padre` (la raíz u otro capítulo).
    pub fn nuevo_capitulo(&mut self, padre: &str, codigo: &str, resumen: &str) -> Result<(), ErrorMotor> {
        self.exigir_capitulo(padre)?;
        self.insertar_nuevo(Concepto::capitulo(codigo, resumen))?;
        self.anadir_linea(padre, codigo, Decimal::ONE).inspect_err(|_| {
            self.conceptos.remove(codigo);
        })
    }

    /// Crea una partida vacía (sin descomposición) con cantidad 1 en `capitulo`.
    pub fn nueva_partida(
        &mut self,
        capitulo: &str,
        codigo: &str,
        unidad: &str,
        resumen: &str,
    ) -> Result<(), ErrorMotor> {
        self.exigir_capitulo(capitulo)?;
        self.insertar_nuevo(Concepto::partida(codigo, unidad, resumen, Vec::new()))?;
        self.anadir_linea(capitulo, codigo, Decimal::ONE).inspect_err(|_| {
            self.conceptos.remove(codigo);
        })
    }

    /// Crea un recurso básico (mano de obra, maquinaria, material, subcontrata,
    /// otros) o un concepto porcentual. No lo cuelga de ningún sitio.
    pub fn nuevo_recurso(
        &mut self,
        codigo: &str,
        unidad: &str,
        resumen: &str,
        naturaleza: Naturaleza,
        precio: Decimal,
    ) -> Result<(), ErrorMotor> {
        if !naturaleza.es_basico() && naturaleza != Naturaleza::Porcentaje {
            return Err(ErrorMotor::ValorInvalido(
                "un recurso debe ser mano de obra, maquinaria, material, subcontrata, otros o porcentaje".into(),
            ));
        }
        if precio < Decimal::ZERO && naturaleza != Naturaleza::Porcentaje {
            return Err(ErrorMotor::ValorInvalido("el precio no puede ser negativo".into()));
        }
        let c = if naturaleza == Naturaleza::Porcentaje {
            Concepto::porcentaje(codigo, resumen)
        } else {
            Concepto::basico(codigo, unidad, resumen, naturaleza, precio)
        };
        self.insertar_nuevo(c)
    }

    /// Añade al final de `padre` una línea con un concepto ya existente.
    ///
    /// Reglas: en un capítulo solo cuelgan capítulos y partidas; en una partida
    /// cualquier concepto salvo capítulos; los recursos básicos no se
    /// descomponen; un mismo hijo no se repite en el mismo padre; no se
    /// permiten referencias circulares (una partida dentro de sí misma).
    pub fn anadir_linea(&mut self, padre: &str, hijo: &str, cantidad: Decimal) -> Result<(), ErrorMotor> {
        let p = self.concepto(padre)?;
        let h = self.concepto(hijo)?;
        if p.naturaleza.es_basico() || p.naturaleza == Naturaleza::Porcentaje {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{padre}» es un recurso básico: no se descompone"
            )));
        }
        let en_capitulo = p.naturaleza == Naturaleza::Capitulo;
        if en_capitulo && !matches!(h.naturaleza, Naturaleza::Capitulo | Naturaleza::Partida) {
            return Err(ErrorMotor::ValorInvalido(format!(
                "en un capítulo solo puede haber capítulos o partidas, y «{hijo}» es un recurso"
            )));
        }
        if !en_capitulo && h.naturaleza == Naturaleza::Capitulo {
            return Err(ErrorMotor::ValorInvalido(format!(
                "un capítulo («{hijo}») no puede formar parte de una partida"
            )));
        }
        if hijo == self.raiz {
            return Err(ErrorMotor::ValorInvalido(
                "la raíz no puede colgar de otro concepto".into(),
            ));
        }
        if p.descomposicion.iter().any(|l| l.hijo == hijo) {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{hijo}» ya está en «{padre}»: cambie su cantidad"
            )));
        }
        self.concepto_mut(padre)?
            .descomposicion
            .push(LineaDescomposicion::new(hijo, cantidad));
        if let Err(e) = self.validar() {
            self.concepto_mut(padre)?.descomposicion.pop();
            return Err(e);
        }
        Ok(())
    }

    /// Quita la línea `hijo` de `padre` (y su hoja de medición, si la tiene).
    /// El concepto hijo no se borra: puede seguir usándose en otros sitios.
    pub fn quitar_linea(&mut self, padre: &str, hijo: &str) -> Result<(), ErrorMotor> {
        let d = &mut self.concepto_mut(padre)?.descomposicion;
        let i = d
            .iter()
            .position(|l| l.hijo == hijo)
            .ok_or_else(|| ErrorMotor::RecursoNoEnPartida {
                partida: padre.to_owned(),
                recurso: hijo.to_owned(),
            })?;
        d.remove(i);
        self.mediciones.remove(&(padre.to_owned(), hijo.to_owned()));
        Ok(())
    }

    /// Quita la línea y borra los conceptos que quedan sin usar (el hijo y,
    /// en cascada, sus descendientes huérfanos). Devuelve los códigos borrados.
    pub fn quitar_y_purgar(&mut self, padre: &str, hijo: &str) -> Result<Vec<String>, ErrorMotor> {
        self.quitar_linea(padre, hijo)?;
        let mut borrados = Vec::new();
        let mut pendientes = vec![hijo.to_owned()];
        while let Some(c) = pendientes.pop() {
            if c == self.raiz || !self.padres(&c).is_empty() {
                continue;
            }
            if let Some(concepto) = self.conceptos.remove(&c) {
                self.mediciones.retain(|(pa, _), _| pa != &c);
                pendientes.extend(concepto.descomposicion.into_iter().map(|l| l.hijo));
                borrados.push(c);
            }
        }
        Ok(borrados)
    }

    /// Sube (`arriba`) o baja una posición la línea `hijo` dentro de `padre`.
    /// El orden importa: los porcentajes se aplican sobre las líneas anteriores.
    pub fn mover_linea(&mut self, padre: &str, hijo: &str, arriba: bool) -> Result<(), ErrorMotor> {
        let d = &mut self.concepto_mut(padre)?.descomposicion;
        let i = d
            .iter()
            .position(|l| l.hijo == hijo)
            .ok_or_else(|| ErrorMotor::ConceptoInexistente(hijo.to_owned()))?;
        let j = if arriba {
            i.checked_sub(1)
        } else {
            (i + 1 < d.len()).then_some(i + 1)
        };
        if let Some(j) = j {
            d.swap(i, j);
        }
        Ok(())
    }

    /// Borra un concepto que no usa nadie. Si se usa, el error dice dónde.
    pub fn borrar_concepto(&mut self, codigo: &str) -> Result<(), ErrorMotor> {
        self.concepto(codigo)?;
        if codigo == self.raiz {
            return Err(ErrorMotor::ValorInvalido(
                "no se puede borrar la raíz del presupuesto".into(),
            ));
        }
        let padres = self.padres(codigo);
        if !padres.is_empty() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» se usa en {}: quite antes esas líneas",
                padres.join(", ")
            )));
        }
        self.conceptos.remove(codigo);
        self.mediciones.retain(|(pa, _), _| pa != codigo);
        Ok(())
    }

    /// Conceptos que contienen a `codigo` en su descomposición.
    pub fn padres(&self, codigo: &str) -> Vec<String> {
        self.conceptos
            .values()
            .filter(|c| c.descomposicion.iter().any(|l| l.hijo == codigo))
            .map(|c| c.codigo.clone())
            .collect()
    }

    /// Cambia la unidad de un concepto (no de los capítulos).
    pub fn fijar_unidad(&mut self, codigo: &str, unidad: &str) -> Result<(), ErrorMotor> {
        let c = self.concepto_mut(codigo)?;
        if c.naturaleza == Naturaleza::Capitulo {
            return Err(ErrorMotor::ValorInvalido("los capítulos no tienen unidad".into()));
        }
        c.unidad = unidad.trim().to_owned();
        Ok(())
    }

    /// Primer código libre de la forma `prefijo` + número (p. ej. «C03», «P0012»).
    pub fn codigo_libre(&self, prefijo: &str, cifras: usize) -> String {
        (1..)
            .map(|n| format!("{prefijo}{n:0cifras$}"))
            .find(|c| !self.conceptos.contains_key(c))
            .unwrap_or_default()
    }

    fn exigir_capitulo(&self, codigo: &str) -> Result<(), ErrorMotor> {
        if self.concepto(codigo)?.naturaleza != Naturaleza::Capitulo {
            return Err(ErrorMotor::ValorInvalido(format!("«{codigo}» no es un capítulo")));
        }
        Ok(())
    }
}
