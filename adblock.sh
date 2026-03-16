#!/bin/bash
#
# Script de téléchargement des listes de filtres pour Servo AdBlock
#
# Usage: ./download_filters.sh [--force]

set -e

FILTER_DIR="resources/adblock"
FORCE_DOWNLOAD=false

# Vérifier les arguments
if [ "$1" == "--force" ]; then
    FORCE_DOWNLOAD=true
fi

# Couleurs pour l'affichage
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Créer le dossier s'il n'existe pas
mkdir -p "$FILTER_DIR"

echo "========================================"
echo "  Téléchargement des listes de filtres"
echo "========================================"
echo ""

# Fonction pour télécharger un filtre
download_filter() {
    local name="$1"
    local url="$2"
    local filename="$3"
    local filepath="$FILTER_DIR/$filename"
    
    # Vérifier si le fichier existe déjà
    if [ -f "$filepath" ] && [ "$FORCE_DOWNLOAD" != "true" ]; then
        echo -e "${YELLOW}⊙${NC} $name déjà présent (utilisez --force pour re-télécharger)"
        return 0
    fi
    
    echo -e "${YELLOW}↓${NC} Téléchargement de $name..."
    
    if curl -f -L -o "$filepath" "$url" 2>/dev/null; then
        local lines=$(wc -l < "$filepath")
        echo -e "${GREEN}✓${NC} $name téléchargé ($lines lignes)"
    else
        echo -e "${RED}✗${NC} Échec du téléchargement de $name"
        return 1
    fi
}

# Liste des filtres à télécharger
download_filter "EasyList" \
    "https://easylist.to/easylist/easylist.txt" \
    "easylist.txt"

download_filter "EasyPrivacy" \
    "https://easylist.to/easylist/easyprivacy.txt" \
    "easyprivacy.txt"

# Filtres additionnels (optionnels mais recommandés)
download_filter "EasyList Cookie" \
    "https://secure.fanboy.co.nz/fanboy-cookiemonster.txt" \
    "fanboy-cookiemonster.txt"

download_filter "Malware Domains" \
    "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-online.txt" \
    "malware.txt"

# Filtres régionaux (exemples - décommenter si nécessaire)

# Français
download_filter "EasyList France" \
    "https://easylist-downloads.adblockplus.org/liste_fr.txt" \
    "liste_fr.txt"

# Allemand
download_filter "EasyList Germany" \
    "https://easylist.to/easylistgermany/easylistgermany.txt" \
    "easylistgermany.txt"

# Chinois
download_filter "EasyList China" \
    "https://easylist-downloads.adblockplus.org/easylistchina.txt" \
    "easylistchina.txt"

# Résumé
echo ""
echo "========================================"
echo "  Résumé"
echo "========================================"

total_files=$(find "$FILTER_DIR" -name "*.txt" | wc -l)
total_rules=0

for file in "$FILTER_DIR"/*.txt; do
    if [ -f "$file" ]; then
        rules=$(grep -v '^!' "$file" | grep -v '^$' | wc -l)
        total_rules=$((total_rules + rules))
    fi
done

echo "Filtres téléchargés: $total_files"
echo "Règles de blocage totales: ~$total_rules"
echo "Emplacement: $FILTER_DIR"
echo ""

# Créer un fichier de configuration par défaut si nécessaire
CONFIG_FILE="$FILTER_DIR/config.json"
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Création du fichier de configuration..."
    cat > "$CONFIG_FILE" << 'EOF'
{
  "enabled": true,
  "filter_lists": [
    "resources/adblock/easylist.txt",
    "resources/adblock/easyprivacy.txt",
    "resources/adblock/fanboy-cookiemonster.txt",
    "resources/adblock/malware.txt",
    "resources/adblock/liste_fr.txt",
    "resources/adblock/easylistegermany.txt",
    "resources/adblock/easylistechina.txt"
  ],
  "filter_urls": [
    {
      "name": "EasyList",
      "url": "https://easylist.to/easylist/easylist.txt",
      "enabled": true
    },
    {
      "name": "EasyPrivacy",
      "url": "https://easylist.to/easylist/easyprivacy.txt",
      "enabled": true
    },
    {
      "name": "EasyList Cookie",
      "url": "https://secure.fanboy.co.nz/fanboy-cookiemonster.txt",
      "enabled": true
    },
    {
      "name": "Malware Domains",
      "url": "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-online.txt",
      "enabled": true
    },
    {
      "name": "EasyList France",
      "url": "https://easylist-downloads.adblockplus.org/liste_fr.txt",
      "enabled": true
    },
    {
      "name": "EasyList Germany",
      "url": "https://easylist-downloads.adblockplus.org/easylistgermany.txt",
      "enabled": true
    },
    {
      "name": "EasyList China",
      "url": "https://easylist-downloads.adblockplus.org/easylistchina.txt",
      "enabled": true
    }
  ],
  "whitelist_domains": [],
  "aggressive_mode": false,
  "auto_update": true,
  "update_interval_hours": 24
}
EOF
    echo -e "${GREEN}✓${NC} Configuration créée: $CONFIG_FILE"
fi

echo -e "${GREEN}✓${NC} Terminé!"
echo ""
echo "Prochaine étape: Compiler Servo avec './mach build --release'"
