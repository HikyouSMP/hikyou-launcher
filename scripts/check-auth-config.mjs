const clientId = (process.env.HIKYOU_MSA_CLIENT_ID || "").trim();
const applicationId = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const placeholderId = "00000000-0000-0000-0000-000000000000";

if (!applicationId.test(clientId) || clientId === placeholderId) {
  console.error(
    "HIKYOU_MSA_CLIENT_ID must contain the approved Microsoft public-client application UUID.",
  );
  process.exit(1);
}

console.log("Microsoft public-client build configuration is present.");
