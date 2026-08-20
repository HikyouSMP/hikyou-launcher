export function ProfileProgressBar({ percent }: { percent: number | null }) {
  const determinate = percent !== null;
  return (
    <div
      aria-hidden="true"
      className={
        determinate
          ? "profile-progress determinate"
          : "profile-progress indeterminate"
      }
    >
      {determinate ? (
        <div
          className="profile-progress-fill"
          style={{
            width: `${percent}%`,
            minWidth: percent > 0 ? 8 : undefined,
          }}
        />
      ) : (
        <span className="profile-progress-segment" />
      )}
    </div>
  );
}
