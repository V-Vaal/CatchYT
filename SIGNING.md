# Signer CatchYT (et supprimer les alertes antivirus / SmartScreen)

Un `.exe` non signé n'est pas « infecté » — il est simplement **sans réputation**. Windows SmartScreen affiche alors « Windows a protégé votre ordinateur / éditeur inconnu », et certains antivirus appliquent une heuristique plus stricte. La signature de code résout ça en liant le binaire à une identité vérifiée.

Ce guide explique **quoi acheter** et **comment signer**. Tu n'as pas besoin de signer pour que l'app fonctionne ; c'est uniquement pour l'expérience de distribution.

---

## 1. Quel certificat choisir

Il existe deux niveaux. La différence pratique tient à la **réputation SmartScreen**.

| | **OV** (Organization Validation) | **EV** (Extended Validation) |
|---|---|---|
| Prix indicatif | ~130–300 €/an | ~250–450 €/an |
| Qui peut l'obtenir | entreprise **et** désormais particuliers/indépendants chez certains fournisseurs | entreprise enregistrée (souvent exigée) |
| Réputation SmartScreen | se construit progressivement (après N installations) | **réputation immédiate**, pas d'avertissement dès le départ |
| Stockage de la clé | fichier `.pfx` ou token matériel (depuis 2023, stockage matériel imposé) | token matériel / HSM obligatoire |

Depuis juin 2023, le CA/Browser Forum impose que **la clé privée soit sur un support matériel** (token USB type YubiKey/SafeNet, ou HSM cloud). Concrètement tu recevras soit un token physique, soit un accès à un service de signature cloud.

**Recommandation :**
- Tu veux zéro avertissement immédiat et tu es prêt à payer / justifier une entité → **EV**.
- Tu es indépendant, budget serré, et tolères que la réputation se construise sur quelques semaines → **OV**.

Fournisseurs courants : **DigiCert**, **Sectigo** (ex-Comodo), **GlobalSign**, **SSL.com**, **Certum** (Certum propose des certificats « Open Source Code Signing » abordables pour les développeurs individuels, avec token — souvent le meilleur rapport qualité/prix pour un projet perso).

---

## 2. Alternatives sans acheter tout de suite

- **Azure Trusted Signing** (Microsoft) : service de signature cloud, tarif à l'usage (~quelques €/mois), pas de token à gérer. Nécessite une organisation éligible. C'est aujourd'hui l'option la plus simple si tu remplis les critères.
- **Ne pas signer** et documenter pour tes utilisateurs comment passer l'avertissement : au lancement, cliquer sur **« Informations complémentaires » → « Exécuter quand même »**. Acceptable pour un usage personnel ou un petit cercle.
- **SmartScreen se calme tout seul** avec le volume de téléchargements pour un binaire OV ; pour un usage perso, l'exe local que **tu** compiles n'a de toute façon presque jamais de souci.

---

## 3. Signer le binaire une fois le certificat obtenu

Avec le SDK Windows (`signtool.exe`) et un horodatage (indispensable pour que la signature reste valide après expiration du certificat) :

```powershell
# .pfx (certificat OV en fichier, si ton fournisseur l'autorise encore)
signtool sign `
  /fd SHA256 `
  /f C:\chemin\vers\certificat.pfx `
  /p "MOT_DE_PASSE_PFX" `
  /tr http://timestamp.digicert.com `
  /td SHA256 `
  target\release\catchyt.exe

# Vérifier
signtool verify /pa /v target\release\catchyt.exe
```

Avec un **token matériel** (OV/EV modernes), le certificat est exposé via le magasin Windows ; on référence alors le sujet plutôt qu'un `.pfx` :

```powershell
signtool sign /fd SHA256 /n "Nom exact du sujet du certificat" `
  /tr http://timestamp.sectigo.com /td SHA256 `
  target\release\catchyt.exe
```

---

## 4. Signer automatiquement dans la CI GitHub

Deux approches :

1. **Azure Trusted Signing** : action officielle `azure/trusted-signing-action`, secrets Azure en variables de dépôt. Le plus propre pour un pipeline.
2. **`.pfx` en secret** (si ton certificat le permet) : stocke le `.pfx` encodé en base64 dans un secret `SIGNING_PFX_BASE64` et le mot de passe dans `SIGNING_PFX_PASSWORD`, puis ajoute une étape après le build :

```yaml
      - name: Sign exe
        shell: pwsh
        run: |
          $pfx = [Convert]::FromBase64String("${{ secrets.SIGNING_PFX_BASE64 }}")
          [IO.File]::WriteAllBytes("cert.pfx", $pfx)
          & "${env:ProgramFiles(x86)}\Windows Kits\10\bin\x64\signtool.exe" sign `
            /fd SHA256 /f cert.pfx /p "${{ secrets.SIGNING_PFX_PASSWORD }}" `
            /tr http://timestamp.digicert.com /td SHA256 `
            target\x86_64-pc-windows-msvc\release\catchyt.exe
          Remove-Item cert.pfx
```

> Les certificats à token matériel ne s'automatisent pas avec un simple `.pfx` : privilégie Azure Trusted Signing pour une CI entièrement automatique.

---

## Résumé

- Usage perso, exe que tu compiles toi-même → **rien à faire**, au pire un clic SmartScreen.
- Distribution large sans avertissement → **certificat EV** (immédiat) ou **Azure Trusted Signing**.
- Budget serré / indépendant → **certificat OV Certum** ou similaire, réputation qui se construit.
- Toujours **horodater** (`/tr` + `/td SHA256`) pour pérenniser la signature.
