
# 🚀 SUPNUM Web Fuzzer

> **Un scanner de répertoires et de fichiers web ultra-rapide, asynchrone et récursif.**

Développé dans le cadre de l'**Institut Supérieur du Numérique** (SUPNUM), cet outil est conçu pour les tests d'intrusion (Pentest), les CTF et la chasse aux bugs. Il utilise la puissance de Rust, `Tokio` et `MiMalloc` pour offrir des performances maximales avec une empreinte mémoire minimale.

---

## ⚡ Fonctionnalités Clés

* **Vitesse Extrême** : Utilise le moteur asynchrone `Tokio` et l'allocateur de mémoire `MiMalloc`.
* **Fuzzing Intelligent** : Support du mot-clé `FUZZ` ou concaténation automatique.
* **Récursivité** : Scanne automatiquement les sous-dossiers découverts (`-r`).
* **Filtrage Avancé** :
* Exclusion par codes HTTP (404, 500, etc.).
* Filtrage par taille de réponse (`--fs`).


* **Extensions Multiples** : Recherche automatique de variantes (`.php`, `.html`, `.txt`, etc.).
* **Interface Moderne** : Barre de progression en temps réel, colorisation et calcul de latence moyenne.
* **Haute Concurrence** : Gestion efficace de centaines de threads simultanés.

---

## 🛠️ Installation

### Prérequis

Vous devez avoir **Rust** et **Cargo** installés sur votre machine.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

```

### Compilation

Clonez le dépôt et compilez en mode `release` pour des performances optimales :

```bash
git clone https://github.com/23092-ctrl/supnum-fuzzer.git
cd supnum-fuzzer
cargo build --release

```

L'exécutable se trouvera dans `target/release/supnum-fuzzer` (ou le nom de votre binaire).

---

## 💻 Utilisation

### Commande de base

```bash
./target/release/scanner -u http://target.com -w wordlist.txt

```

### Options Disponibles

| Flag | Description | Défaut | Exemple |
| --- | --- | --- | --- |
| `-u, --url` | URL cible (supporte le mot-clé `FUZZ`). | **Requis** | `http://site.com/FUZZ` |
| `-w, --wordlist` | Chemin vers le fichier de liste de mots. | **Requis** | `/usr/share/wordlists/dirb/common.txt` |
| `-t, --threads` | Nombre de requêtes simultanées. | `100` | `-t 200` |
| `-x, --extensions` | Extensions à ajouter aux mots (séparées par des virgules). | Aucune | `-x php,json,bak` |
| `-r, --recurse` | Profondeur de récursivité (si un dossier est trouvé). | `1` | `-r 3` |
| `-e, --exclude` | Codes HTTP à ignorer. | `404` | `-e 404,403,500` |
| `--fs` | Filtrer les réponses par taille (en octets). | Aucun | `--fs 0,1234` |

---

## 🔥 Exemples

### 1. Scan simple avec extensions

Recherche des fichiers `.php` et `.txt` sur la cible :

```bash
cargo run --release -- -u http://10.10.10.15 -w common.txt -x php,txt

```

### 2. Mode Fuzzing précis

Injecte les mots de la liste à un endroit précis de l'URL :

```bash
cargo run --release -- -u http://api.target.com/v1/user/FUZZ/details -w ids.txt

```

### 3. Scan agressif et récursif

Utilise 200 threads, descend de 3 niveaux dans les dossiers trouvés et ignore les erreurs 403 et 404 :

```bash
cargo run --release -- -u http://target.com -w big.txt -t 200 -r 3 -e 404,403

```

### 4. Filtrage des faux positifs

Si toutes les réponses font 1540 octets (page d'erreur générique), filtrez-les :

```bash
cargo run --release -- -u http://target.com -w wordlist.txt --fs 1540

```

---

## 📷 Aperçu

```text
         111111111    11      11    111111011
        11            11      11    11     10
        11            11      11    11     10
         11111111     11      11    111110101
                11    11      11    11
                11    11      11    11
        111111111      10010111     11

        11      11    11      11    11      11
        111     11    11      11    111    010
        11 11   11    11      11    11 1111 10
        11  11  11    11      11    11  10  01
        11   11 11    11      11    11      10
        11     111    11      11    11      01
        11      11     11111111     11      10

        Institut Supérieur du Numérique — by Cheikh ELghadi
        GitHub : https://github.com/23092-ctrl
------------------------------------------------------------

🚀 Scan Ultra-Rapide (Zéro Attente au Lancement)
⠴ [00:00:12] 4502 reqs (350/s) | Latence: 45ms
[200]     4096 | http://target.com/admin
[301]      178 | http://target.com/images
[200]     8192 | http://target.com/index.php

```

---

## ⚠️ Avertissement Légal

Cet outil est développé à des fins éducatives et pour les tests de sécurité autorisés. L'auteur et l'Institut Supérieur du Numérique (SUPNUM) déclinent toute responsabilité en cas d'utilisation abusive ou illégale sur des systèmes pour lesquels vous ne disposez pas d'une autorisation explicite.

---

## 👤 Auteur

* **Cheikh ELghadi**
* **GitHub** : [23092-ctrl](https://github.com/23092-ctrl)
* **Institut** : SUPNUM (Mauritanie)
