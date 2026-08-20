const MSI_CHANNEL_OFFSETS = {
  alpha: 10_000,
  beta: 30_000,
  rc: 50_000,
};

const MSI_MAX_MAJOR_OR_MINOR = 255;
const MSI_MAX_PATCH_OR_BUILD = 65_535;
const MSI_CHANNEL_SEQUENCE_LIMIT = 9_999;

/**
 * Derive the Windows Installer version from the public SemVer application
 * version. Pre-releases are placed below their eventual stable release.
 */
export function deriveWixVersion(version) {
  const match = version.match(
    /^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc)\.(\d+))?(?:\+[0-9A-Za-z.-]+)?$/,
  );

  if (!match) {
    throw new Error(
      "MSI packaging supports stable versions or alpha.N, beta.N, and rc.N pre-releases.",
    );
  }

  const [, majorText, minorText, patchText, channel, sequenceText] = match;
  const [major, minor, patch] = [majorText, minorText, patchText].map(Number);
  validateMsiComponent(major, MSI_MAX_MAJOR_OR_MINOR, "major");
  validateMsiComponent(minor, MSI_MAX_MAJOR_OR_MINOR, "minor");
  validateMsiComponent(patch, MSI_MAX_PATCH_OR_BUILD, "patch");

  if (!channel) {
    return `${major}.${minor}.${patch}`;
  }

  const sequence = Number(sequenceText);
  if (!Number.isInteger(sequence) || sequence > MSI_CHANNEL_SEQUENCE_LIMIT) {
    throw new Error(
      `MSI pre-release sequence must be between 0 and ${MSI_CHANNEL_SEQUENCE_LIMIT}.`,
    );
  }

  const [preMajor, preMinor, prePatch] = predecessorVersion(major, minor, patch);
  const build = MSI_CHANNEL_OFFSETS[channel] + sequence;
  return `${preMajor}.${preMinor}.${prePatch}.${build}`;
}

function predecessorVersion(major, minor, patch) {
  if (patch > 0) {
    return [major, minor, patch - 1];
  }
  if (minor > 0) {
    return [major, minor - 1, MSI_MAX_PATCH_OR_BUILD];
  }
  if (major > 0) {
    return [major - 1, MSI_MAX_MAJOR_OR_MINOR, MSI_MAX_PATCH_OR_BUILD];
  }
  throw new Error("Cannot derive an MSI version below 0.0.0 for a pre-release.");
}

function validateMsiComponent(value, maximum, name) {
  if (!Number.isInteger(value) || value < 0 || value > maximum) {
    throw new Error(`MSI ${name} version component must be between 0 and ${maximum}.`);
  }
}
