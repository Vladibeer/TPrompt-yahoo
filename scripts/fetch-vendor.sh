#!/usr/bin/env bash
# Сваля библиотеките, които index.html зарежда локално при нужда:
#  - jsQR (сканиране на QR код на телефони без вграден скенер)
#  - mammoth (отваряне на .docx)
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p www/vendor
curl -fsSL "https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js" -o www/vendor/jsQR.js
curl -fsSL "https://cdn.jsdelivr.net/npm/mammoth@1.6.0/mammoth.browser.min.js" -o www/vendor/mammoth.browser.min.js
echo "Готово: www/vendor"
