# TraceTiroir

Logiciel libre, hors ligne et sans compte, qui transforme une photo d'objets posés sur une feuille
blanche en plateau imprimable en 3D avec leurs empreintes (alternative à Tool Trace).

## Utilisation

1. Posez les objets sur une feuille blanche (A4 par défaut), sur un fond plus sombre.
2. Photographiez **du dessus**, feuille entière visible, de préférence de loin avec zoom
   (moins de perspective) et en lumière diffuse (pas d'ombres portées).
3. Lancez `tracetiroir.exe`, ouvrez ou glissez-déposez la photo.
   - **Onglet 1** : vérifiez/ajustez les 4 coins de la feuille.
   - **Onglet 2** : contours détectés (rouge). Cliquez pour ajouter des encoches de préhension,
     ou passez en mode « ignorer objet » pour exclure un élément.
   - **Onglet 3** : aperçu 3D (glisser pour tourner).
4. Exportez en **STL** (impression), **SVG** ou **DXF** (découpe laser / CNC / mousse).

### Réglages principaux
| Réglage | Rôle |
|---|---|
| seuil sombre / couleur | sensibilité de la détection (ombres vs objets clairs) |
| jeu autour | marge entre l'objet et la paroi (1 mm par défaut) |
| dimensions imposées | plaque simple : largeur × longueur fixées (ex. fond de tiroir), empreintes centrées ; `--taille LxW` en ligne de commande |
| profondeur par objet | chaque objet (numéroté sur l’image) peut avoir sa propre profondeur ; `--objet N=prof` en ligne de commande |
| profondeur / fond / bordure | géométrie de la plaque |
| Gridfinity | bac au pas de 42 mm avec pieds standard (sans rebord d'empilement) |

## Ligne de commande

```
tracetiroir --cli photo.jpg sortie.stl [--gridfinity] [--profondeur 15] [--jeu 1] [--taille 300x220] [--objet 2=25] [--papier a4|a3|a5|letter|legal]
```
Produit aussi `sortie.svg`, `sortie.controle.png` (contours) et `sortie.apercu.png` (3D).

On peut aussi lancer `tracetiroir photo.jpg` (ou glisser la photo sur l’exécutable) pour l’ouvrir directement dans l’interface.

`tracetiroir --demo test.jpg` génère une photo synthétique de test.

## Compilation

```
cargo build --release
```
L'exécutable est dans `target/release/tracetiroir.exe`.

## Fonctionnement

1. Feuille : seuillage d'Otsu → plus grande zone claire → enveloppe convexe → quadrilatère d'aire maximale.
2. Redressement par homographie à 8 px/mm (l'échelle vient de la taille connue de la feuille).
3. Segmentation : pixel plus sombre que le fond local estimé (corrige l'éclairage non uniforme)
   ou saturé (objets colorés) ; fermeture morphologique, trous bouchés, filtrage par surface.
4. Jeu : dilatation par distance euclidienne exacte, union des encoches, extraction des contours,
   simplification Douglas-Peucker + lissage de Chaikin.
5. Maillage étanche : triangulation (earcut) des faces, parois verticales, pieds Gridfinity.

## Licence

MIT, voir [LICENSE](LICENSE).
