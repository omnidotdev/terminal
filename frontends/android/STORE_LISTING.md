# Play Store Listing Checklist

## Required Assets
- [ ] App icon: 512x512 PNG (high-res, no transparency)
- [ ] Feature graphic: 1024x500 PNG
- [ ] Screenshots: minimum 2, recommended 4-8 (phone + tablet)
  - Connect screen
  - Local shell session
  - Multi-tab view
  - Arch Linux environment
  - Pinch-to-zoom / text selection
- [x] Privacy policy URL: https://omni.dev/privacy-policy

## Store Listing
- [x] Title (30 char max): "Omni Terminal" — see fastlane/
- [x] Short description (80 char max): see fastlane/
- [x] Full description (4000 char max): see fastlane/
- [ ] Category: Tools
- [ ] Content rating: complete questionnaire (no objectionable content)
- [ ] Contact email: support@omni.dev

## Special Declarations
- [ ] MANAGE_EXTERNAL_STORAGE justification: "Terminal emulator that provides command-line access to user files (Downloads, Pictures, etc.) for file management operations"
- [ ] Foreground service justification: "Maintains active terminal sessions while app is in background to prevent data loss"

## Release
- [x] Generate signing keystore (omni-terminal.jks) — BACK UP TO PASSWORD VAULT
- [x] Create keystore.properties
- [ ] Build release AAB: ./gradlew bundleRelease
- [ ] Test release build on device
- [ ] Create closed testing track first (recommended)
