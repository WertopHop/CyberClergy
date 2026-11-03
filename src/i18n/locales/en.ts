// English source of truth for the interface copy. Every other locale is typed
// against this object, so a missing key fails the build instead of rendering a
// raw identifier.
export const en = {
  appName: "CyberClergy",
  menu: "Confessional",
  minimize: "Minimize",
  maximize: "Maximize",
  restore: "Restore",
  close: "Close",
  desktopOnly: "Available in the desktop app",

  confessionalTitle: "Confessional",
  confessionalHint: "What weighs on you",
  confessionalEmpty: "Nothing confessed yet.",
  confessionalPlaceholder: "Write what you would like to be prayed for…",
  confessionalAdd: "Add",
  confessionalRemove: "Remove entry",
  confessionalPending: "waiting",
  confessionalNote: "Entries stay on this device. The clergy will pray over them once the scheduler is connected.",
  settings: "Settings",

  chapelMeta: "{count} clerics",
  chapelEmptyTitle: "No network is connected",
  chapelEmptyBody: "Add at least one API key and the clergy will begin to pray.",
  chapelEmptyAction: "Open settings",
  chapelError: "The prayer did not arrive",
  chapelRetry: "Try again",
  chapelPraying: "praying",
  chapelSilent: "silent",

  settingsTitle: "Settings",
  settingsClose: "Close settings",
  settingsNetworks: "Networks",
  settingsNetworksHint: "Every network is optional. Keys are stored on this computer.",
  settingsAppearance: "Appearance",
  settingsChapel: "Chapel",
  theme: "Theme",
  themeNight: "Night",
  themeDawn: "Dawn",
  language: "Language",
  languageRussian: "Русский",
  languageEnglish: "English",
  religion: "Tradition",
  clerics: "Clerics on the cloud",

  providerOpenRouter: "OpenRouter",
  providerGemini: "Google AI Studio",
  providerGroq: "Groq",
  providerMistral: "Mistral AI",
  providerCloudflare: "Cloudflare Workers AI",
  providerNvidia: "NVIDIA NIM",
  configured: "Connected",
  notConfigured: "Not configured",
  fieldApiKey: "API key",
  fieldAccountId: "Account ID",
  fieldToken: "API token",
  save: "Save",
  saved: "Saved",
  forget: "Forget",
  check: "Check",
  checking: "Checking…",
  checkOk: "Responded in {ms} ms",
  checkFailed: "Failed: {message}",
  modelLine: "Model: {model}",
  placeholderSet: "Already set — enter a new value to replace it",
  placeholderEmpty: "Paste the value from the provider dashboard",

  previewJudaism:
    "Sovereign of the universe, hold these small hands steady tonight. Let the worries of your children soften into rest, and let our tefillah rise as one quiet voice before you.",
  previewChristianity:
    "Lord, we lift these small hands for the one who cannot yet speak. Steady their steps, quiet their fears, and let them know they are not alone tonight. Amen.",
  previewIslam:
    "O Most Merciful, we raise these voices for one who asked for help without naming the need. Grant them ease after hardship, patience in the waiting, and a heart at rest. Amin.",
} satisfies Record<string, string>;

export type Dictionary = typeof en;
