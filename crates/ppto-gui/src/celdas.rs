// SPDX-License-Identifier: GPL-3.0-or-later
//! Celdas numéricas editables con formato español.
//!
//! Mientras la celda tiene el foco se edita el texto libremente; al salir
//! (Intro, Tab o clic fuera) se interpreta y, si es válido y distinto, se
//! devuelve el nuevo valor. Si no es válido se descarta y vuelve el anterior.

use eframe::egui::{self, Align, TextEdit};
use ppto_core::Decimal;
use ppto_core::formato::{leer, num};

#[derive(Default)]
pub struct Edicion {
    clave: Option<String>,
    texto: String,
}

impl Edicion {
    fn celda(&mut self, ui: &mut egui::Ui, clave: &str, mostrado: String, ancho: f32) -> Option<String> {
        let editando = self.clave.as_deref() == Some(clave);
        let mut texto = if editando { self.texto.clone() } else { mostrado };
        let r = ui.add(
            TextEdit::singleline(&mut texto)
                .id(ui.make_persistent_id(clave))
                .desired_width(ancho)
                .horizontal_align(Align::RIGHT),
        );
        if r.has_focus() {
            self.clave = Some(clave.to_owned());
            self.texto = texto;
            None
        } else if r.lost_focus() || editando {
            self.clave = None;
            Some(texto)
        } else {
            None
        }
    }

    /// Celda obligatoria. Devuelve `Some(nuevo)` al confirmar un valor válido distinto.
    pub fn decimal(&mut self, ui: &mut egui::Ui, clave: &str, valor: Decimal, dec: u32, ancho: f32) -> Option<Decimal> {
        let t = self.celda(ui, clave, num(valor, dec), ancho)?;
        leer(&t).filter(|v| *v != valor)
    }

    /// Celda que admite quedar vacía. Devuelve `Some(nuevo)` al confirmar un cambio.
    pub fn opcional(
        &mut self,
        ui: &mut egui::Ui,
        clave: &str,
        valor: Option<Decimal>,
        dec: u32,
        ancho: f32,
    ) -> Option<Option<Decimal>> {
        let mostrado = valor.map(|v| num(v, dec)).unwrap_or_default();
        let t = self.celda(ui, clave, mostrado, ancho)?;
        let nuevo = if t.trim().is_empty() { None } else { Some(leer(&t)?) };
        (nuevo != valor).then_some(nuevo)
    }
}

#[cfg(test)]
mod tests {
    //! Simula el teclado directamente sobre egui (sin ventana) para comprobar
    //! el ciclo completo: foco → seleccionar todo → escribir → Intro.
    use super::*;
    use eframe::egui::{Context, Event, Key, Modifiers, RawInput};
    use rust_decimal_macros::dec;

    fn tecla(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Escribe `texto` en la celda y pulsa Intro; devuelve lo que confirma la celda.
    fn escribir(valor: Decimal, texto: &str) -> Option<Decimal> {
        let ctx = Context::default();
        let mut ed = Edicion::default();
        let mut confirmado = None;
        let fotograma = |eventos: Vec<Event>, enfocar: bool, ed: &mut Edicion, confirmado: &mut Option<Decimal>| {
            let entrada = RawInput {
                events: eventos,
                ..Default::default()
            };
            let mut salida = ctx.run_ui(entrada, |ui| {
                if enfocar {
                    let id = ui.make_persistent_id("celda");
                    ui.memory_mut(|m| m.request_focus(id));
                }
                if let Some(v) = ed.decimal(ui, "celda", valor, 2, 80.0) {
                    *confirmado = Some(v);
                }
            });
            salida.textures_delta.clear();
        };
        fotograma(vec![], false, &mut ed, &mut confirmado);
        fotograma(vec![], true, &mut ed, &mut confirmado);
        fotograma(vec![tecla(Key::A, Modifiers::COMMAND)], false, &mut ed, &mut confirmado);
        fotograma(vec![Event::Text(texto.into())], false, &mut ed, &mut confirmado);
        fotograma(
            vec![tecla(Key::Enter, Modifiers::NONE)],
            false,
            &mut ed,
            &mut confirmado,
        );
        fotograma(vec![], false, &mut ed, &mut confirmado);
        confirmado
    }

    #[test]
    fn confirma_valor_con_coma_decimal() {
        assert_eq!(escribir(dec!(12.50), "13,25"), Some(dec!(13.25)));
    }

    #[test]
    fn ignora_texto_no_valido_y_valor_igual() {
        assert_eq!(escribir(dec!(12.50), "abc"), None);
        assert_eq!(escribir(dec!(12.50), "12,50"), None);
    }
}
