import logo from "@/assets/brand/logo.png";

/** The application mark: the green folded B with the download arrow. */
export function AppMark({ className = "size-4" }: { className?: string }) {
  return (
    <img
      src={logo}
      alt=""
      draggable={false}
      className={`${className} shrink-0`}
    />
  );
}
