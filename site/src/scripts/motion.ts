import Lenis from "lenis";
import { animate } from "motion/mini";

document.documentElement.classList.add("is-motion-ready");

// Lenis honours prefers-reduced-motion itself and falls back to native scroll.
new Lenis({
  autoRaf: true,
  anchors: true,
  lerp: 0.1,
  stopInertiaOnNavigate: true,
});

// The head script only sets this class when motion is allowed, so it doubles
// as the reduced-motion gate.
if (document.documentElement.classList.contains("js-reveal")) {
  const styles = getComputedStyle(document.documentElement);
  const duration =
    (parseFloat(styles.getPropertyValue("--duration-reveal")) || 720) / 1000;
  const ease = (
    styles.getPropertyValue("--ease-spring").match(/[\d.]+/g) ?? [
      "0.32",
      "0.72",
      "0",
      "1",
    ]
  ).map(Number) as [number, number, number, number];

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;

        observer.unobserve(entry.target);
        // No "from" values: motion starts from the hidden state in the CSS.
        animate(
          entry.target,
          { opacity: 1, transform: "translate3d(0, 0, 0)" },
          { duration, ease },
        );
      }
    },
    { threshold: 0.08, rootMargin: "0px 0px -8% 0px" },
  );

  for (const element of document.querySelectorAll(".reveal")) {
    observer.observe(element);
  }
}
