import type { ReactNode } from "react";
import { useAvatarUrl } from "../avatarCache";

export function AccountAvatar({
  skinId,
  size,
  fallback,
}: {
  skinId: string | null | undefined;
  size: number;
  fallback: ReactNode;
}) {
  const url = useAvatarUrl(skinId ?? null);
  if (!url) return <>{fallback}</>;
  return (
    <img
      src={url}
      width={size}
      height={size}
      className="block"
      style={{ imageRendering: "pixelated" }}
    />
  );
}
