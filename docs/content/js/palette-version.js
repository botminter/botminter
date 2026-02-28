// Clear stale palette preferences when site config changes.
// Bump PALETTE_VERSION whenever mkdocs.yml palette config is modified.
(function () {
  var PALETTE_VERSION = 2;
  var scope = new URL(".", location);
  var key = scope.pathname + ".__palette";
  var versionKey = scope.pathname + ".__palette_v";
  var stored = localStorage.getItem(versionKey);
  if (stored !== String(PALETTE_VERSION)) {
    localStorage.removeItem(key);
    localStorage.setItem(versionKey, String(PALETTE_VERSION));
  }
})();
