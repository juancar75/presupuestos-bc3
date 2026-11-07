-- SPDX-License-Identifier: GPL-3.0-or-later
-- Escalones de un descuento encadenado («40+8»); el porcentaje es su equivalente.
ALTER TABLE descuentos ADD COLUMN cadena TEXT;
