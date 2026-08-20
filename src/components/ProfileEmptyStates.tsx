import { useTranslation } from "react-i18next";

import { C } from "../theme";

export function EmptyProfileHint() {
  const { t } = useTranslation();
  return (
    <div style={{ padding: "44px 24px", textAlign: "center" }}>
      <p
        style={{
          fontSize: 13,
          fontWeight: 500,
          color: C.t2,
          marginBottom: 8,
        }}
      >
        {t("profile.empty_title")}
      </p>
      <p style={{ fontSize: 12, color: C.t3, lineHeight: 1.7 }}>
        {t("profile.empty_hint_pre")}{" "}
        <span style={{ color: C.green, fontWeight: 600 }}>1.21</span>{" "}
        {t("profile.empty_hint_post")}
      </p>
    </div>
  );
}

export function NoProfileMatch({ query }: { query: string }) {
  const { t } = useTranslation();
  return (
    <div style={{ padding: "30px 24px", textAlign: "center" }}>
      <p style={{ fontSize: 12, color: C.t3 }}>
        {t("profile.no_match", { query })}
      </p>
    </div>
  );
}
