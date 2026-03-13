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
- [ ] Privacy policy URL (host at terminal.omni.dev/privacy)

## Store Listing
- [ ] Title (30 char max): "Omni Terminal"
- [ ] Short description (80 char max): see fastlane/
- [ ] Full description (4000 char max): see fastlane/
- [ ] Category: Tools
- [ ] Content rating: complete questionnaire (no objectionable content)
- [ ] Contact email: required

## Special Declarations
- [ ] MANAGE_EXTERNAL_STORAGE justification: "Terminal emulator that provides command-line access to user files (Downloads, Pictures, etc.) for file management operations"
- [ ] Foreground service justification: "Maintains active terminal sessions while app is in background to prevent data loss"

## Release
- [ ] Generate signing keystore
- [ ] Build release AAB: ./gradlew bundleRelease
- [ ] Test release build on device
- [ ] Create closed testing track first (recommended)
