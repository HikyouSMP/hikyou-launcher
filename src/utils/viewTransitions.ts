export type NavDirection = "none" | "back" | "forward";
export type EnterFrom = "left" | "right";
export type RouteMotion = "fade" | "from-left" | "from-right";

export function routeMotionClass(motion: RouteMotion) {
  return `route-motion route-${motion}`;
}

export function mainViewMotion(navDirection: NavDirection): RouteMotion {
  if (navDirection === "none") return "fade";
  return navDirection === "back" ? "from-right" : "from-left";
}

export function nestedViewMotion(navDirection: NavDirection): RouteMotion {
  return navDirection === "back" ? "from-left" : "from-right";
}

export function nestedViewEnterClass(navDirection: NavDirection) {
  return routeMotionClass(nestedViewMotion(navDirection));
}

export function enterFromClass(enterFrom: EnterFrom) {
  return routeMotionClass(enterFrom === "left" ? "from-left" : "from-right");
}
