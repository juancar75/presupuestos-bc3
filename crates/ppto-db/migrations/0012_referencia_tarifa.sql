-- SPDX-License-Identifier: GPL-3.0-or-later
-- Referencia del artículo en la tarifa del fabricante, para pasar a la tarifa nueva.
ALTER TABLE conceptos ADD COLUMN tarifa_referencia TEXT;
