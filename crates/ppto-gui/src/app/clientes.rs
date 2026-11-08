// SPDX-License-Identifier: GPL-3.0-or-later
//! Clientes: los datos de cada cliente se escriben una vez y se eligen en
//! cada presupuesto (PDF de oferta), sin copiarlos de una obra a otra.
//!
//! Se guardan en `clientes.json` de la carpeta de configuración del
//! usuario, **nunca en el repositorio**. Ventana en Obra → Clientes….

use super::Aplicacion;
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cliente {
    /// Razón social o nombre con que sale en la oferta.
    pub nombre: String,
    pub cif: String,
    pub direccion: String,
    pub contacto: String,
    pub telefono: String,
    pub email: String,
    pub forma_pago: String,
    /// Condiciones u observaciones habituales con este cliente (una por línea).
    pub observaciones: String,
}

impl Cliente {
    /// Líneas de observaciones para el PDF: forma de pago y las habituales.
    pub fn lineas_observaciones(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.forma_pago.trim().is_empty() {
            v.push(format!(
                "Forma de pago: {}.",
                self.forma_pago.trim().trim_end_matches('.')
            ));
        }
        v.extend(
            self.observaciones
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
        );
        v
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clientes {
    #[serde(default)]
    pub lista: Vec<Cliente>,
}

pub(super) fn ruta() -> Option<PathBuf> {
    super::pdf::carpeta_config().map(|c| c.join("clientes.json"))
}

fn plano(s: &str) -> String {
    ppto_biblioteca::plano(s)
}

impl Clientes {
    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        if let Some(c) = ruta.parent() {
            std::fs::create_dir_all(c)?;
        }
        let t = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(ruta, t)
    }

    /// Clientes cuyo nombre, CIF o contacto contienen el texto (todos si está vacío).
    pub fn buscar(&self, texto: &str) -> Vec<usize> {
        let t = plano(texto.trim());
        let mut v: Vec<usize> = (0..self.lista.len())
            .filter(|&i| {
                let c = &self.lista[i];
                t.is_empty() || plano(&format!("{} {} {}", c.nombre, c.cif, c.contacto)).contains(&t)
            })
            .collect();
        v.sort_by_key(|&i| plano(&self.lista[i].nombre));
        v
    }

    /// Cliente con ese nombre (sin distinguir mayúsculas ni tildes).
    pub fn por_nombre(&self, nombre: &str) -> Option<&Cliente> {
        let n = plano(nombre.trim());
        self.lista.iter().find(|c| plano(c.nombre.trim()) == n)
    }

    /// Añade un cliente; si ya hay uno con el mismo nombre, lo sustituye.
    pub fn guardar_cliente(&mut self, c: Cliente) {
        let n = plano(c.nombre.trim());
        match self.lista.iter_mut().find(|x| plano(x.nombre.trim()) == n) {
            Some(x) => *x = c,
            None => self.lista.push(c),
        }
    }
}

pub(super) struct VentanaClientes {
    ruta: Option<PathBuf>,
    pub clientes: Clientes,
    buscar: String,
    /// Índice del cliente que se edita (`None`: uno nuevo).
    elegido: Option<usize>,
    pub edicion: Cliente,
    borrar: bool,
    mensaje: Option<(String, bool)>,
}

impl VentanaClientes {
    pub fn abrir() -> Self {
        Self::en(ruta())
    }

    pub fn en(ruta: Option<PathBuf>) -> Self {
        let clientes = ruta.as_deref().map(Clientes::cargar).unwrap_or_default();
        Self {
            ruta,
            clientes,
            buscar: String::new(),
            elegido: None,
            edicion: Cliente::default(),
            borrar: false,
            mensaje: None,
        }
    }

    fn escribir(&mut self) {
        let r = match &self.ruta {
            Some(r) => self.clientes.guardar(r),
            None => Ok(()),
        };
        if let Err(e) = r {
            self.mensaje = Some((format!("No se pudo guardar: {e}"), true));
        }
    }

    pub fn guardar_edicion(&mut self) {
        if self.edicion.nombre.trim().is_empty() {
            self.mensaje = Some(("Escriba el nombre del cliente.".into(), true));
            return;
        }
        let c = self.edicion.clone();
        match self.elegido {
            Some(i) if i < self.clientes.lista.len() => self.clientes.lista[i] = c,
            _ => self.clientes.guardar_cliente(c),
        }
        self.elegido = self
            .clientes
            .lista
            .iter()
            .position(|x| plano(&x.nombre) == plano(&self.edicion.nombre));
        self.mensaje = Some((format!("Cliente «{}» guardado.", self.edicion.nombre.trim()), false));
        self.escribir();
    }

    fn borrar_elegido(&mut self) {
        if let Some(i) = self.elegido.filter(|&i| i < self.clientes.lista.len()) {
            let c = self.clientes.lista.remove(i);
            self.mensaje = Some((format!("Cliente «{}» borrado.", c.nombre), false));
            self.elegido = None;
            self.edicion = Cliente::default();
            self.escribir();
        }
        self.borrar = false;
    }
}

fn campo(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String, pista: &str) {
    ui.label(etiqueta);
    ui.add(egui::TextEdit::singleline(valor).desired_width(380.0).hint_text(pista));
    ui.end_row();
}

impl Aplicacion {
    pub(super) fn ventana_clientes(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.ventana_clientes.take() else {
            return;
        };
        let mut abierta = true;
        egui::Window::new("Clientes")
            .open(&mut abierta)
            .default_width(820.0)
            .default_height(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(r) = &v.ruta {
                    ui.label(
                        RichText::new(format!(
                            "Fichero: {} (solo en este equipo, fuera del repositorio)",
                            r.display()
                        ))
                        .small()
                        .weak(),
                    );
                }
                if let Some((m, error)) = &v.mensaje {
                    let c = if *error {
                        Color32::from_rgb(220, 80, 60)
                    } else {
                        Color32::from_rgb(80, 175, 110)
                    };
                    ui.label(RichText::new(m).color(c));
                }
                ui.horizontal_top(|ui| {
                    // Lista
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        ui.add(egui::TextEdit::singleline(&mut v.buscar).hint_text("buscar nombre, CIF, contacto…"));
                        if ui.button("+ Cliente nuevo").clicked() {
                            v.elegido = None;
                            v.edicion = Cliente::default();
                            v.borrar = false;
                        }
                        egui::ScrollArea::vertical().id_salt("lista_clientes").show(ui, |ui| {
                            let mut elegir = None;
                            for i in v.clientes.buscar(&v.buscar) {
                                let c = &v.clientes.lista[i];
                                if ui.selectable_label(v.elegido == Some(i), &c.nombre).clicked() {
                                    elegir = Some(i);
                                }
                            }
                            if let Some(i) = elegir {
                                v.elegido = Some(i);
                                v.edicion = v.clientes.lista[i].clone();
                                v.borrar = false;
                            }
                            if v.clientes.lista.is_empty() {
                                ui.label(RichText::new("Aún no hay clientes.").weak());
                            }
                        });
                    });
                    ui.separator();
                    // Ficha
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(if v.elegido.is_some() {
                                "Datos del cliente"
                            } else {
                                "Cliente nuevo"
                            })
                            .strong(),
                        );
                        let e = &mut v.edicion;
                        egui::Grid::new("ficha_cliente")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                campo(ui, "Nombre / razón social", &mut e.nombre, "como sale en la oferta");
                                campo(ui, "CIF", &mut e.cif, "");
                                campo(ui, "Dirección", &mut e.direccion, "");
                                campo(ui, "Persona de contacto", &mut e.contacto, "");
                                campo(ui, "Teléfono", &mut e.telefono, "");
                                campo(ui, "Correo", &mut e.email, "");
                                campo(ui, "Forma de pago", &mut e.forma_pago, "p. ej. confirming 90 días");
                            });
                        ui.label("Condiciones u observaciones habituales (una por línea; se pueden poner en el PDF):");
                        ui.add(
                            egui::TextEdit::multiline(&mut e.observaciones)
                                .desired_width(560.0)
                                .desired_rows(5),
                        );
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("Guardar").strong()).clicked() {
                                v.guardar_edicion();
                            }
                            if v.elegido.is_some() {
                                if !v.borrar {
                                    if ui.button("Borrar…").clicked() {
                                        v.borrar = true;
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("¿Borrar este cliente?").color(Color32::from_rgb(220, 80, 60)),
                                    );
                                    if ui.button("Sí, borrar").clicked() {
                                        v.borrar_elegido();
                                    }
                                    if ui.button("No").clicked() {
                                        v.borrar = false;
                                    }
                                }
                            }
                        });
                    });
                });
            });
        if abierta {
            self.ventana_clientes = Some(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acme() -> Cliente {
        Cliente {
            nombre: "Constructora Acme".into(),
            cif: "B00000000".into(),
            forma_pago: "confirming 90 días".into(),
            observaciones: "Oferta válida 30 días\n\nNo incluye ayudas de albañilería".into(),
            ..Default::default()
        }
    }

    #[test]
    fn guardar_buscar_y_observaciones() {
        let dir = std::env::temp_dir().join(format!("clientes-{}", std::process::id()));
        let r = dir.join("clientes.json");
        let mut c = Clientes::default();
        c.guardar_cliente(acme());
        c.guardar_cliente(Cliente {
            nombre: "Faba Obras".into(),
            ..Default::default()
        });
        // Mismo nombre (sin tildes ni mayúsculas): se sustituye
        c.guardar_cliente(Cliente {
            nombre: "CONSTRUCTORA ACME".into(),
            cif: "B11111111".into(),
            ..Default::default()
        });
        assert_eq!(c.lista.len(), 2);
        assert_eq!(c.por_nombre("constructora acme").unwrap().cif, "B11111111");
        assert_eq!(c.buscar("faba").len(), 1);
        assert_eq!(c.buscar("").len(), 2);
        c.guardar(&r).unwrap();
        assert_eq!(Clientes::cargar(&r), c);
        assert_eq!(
            acme().lineas_observaciones(),
            vec![
                "Forma de pago: confirming 90 días.".to_owned(),
                "Oferta válida 30 días".into(),
                "No incluye ayudas de albañilería".into()
            ]
        );
        // Ventana: guardar y borrar
        let mut v = VentanaClientes::en(Some(r.clone()));
        assert_eq!(v.clientes.lista.len(), 2);
        v.edicion = Cliente::default();
        v.guardar_edicion();
        assert!(v.mensaje.as_ref().unwrap().1);
        v.edicion = acme();
        v.guardar_edicion();
        assert_eq!(
            Clientes::cargar(&r).por_nombre("Constructora Acme").unwrap().cif,
            "B00000000"
        );
        v.borrar_elegido();
        assert!(Clientes::cargar(&r).por_nombre("Constructora Acme").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
