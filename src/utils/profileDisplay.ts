export function loaderBadgeClass(loader: string): string {
  switch (loader) {
    case "fabric":
      return "badge-f";
    case "quilt":
      return "badge-q";
    case "neoforge":
      return "badge-n";
    case "forge":
      return "badge-fg";
    case "auto":
      return "badge-auto";
    default:
      return "badge-v";
  }
}

export function loaderDisplayLabel(loader: string): string {
  switch (loader) {
    case "fabric":
      return "Fabric";
    case "quilt":
      return "Quilt";
    case "neoforge":
      return "NeoForge";
    case "forge":
      return "Forge";
    case "auto":
      return "Auto";
    default:
      return "Vanilla";
  }
}
