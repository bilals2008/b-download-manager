/**
 * The sound that says a download is finished.
 *
 * The three tones are built from an oscillator rather than shipped as an audio
 * file: no extra bytes in the build, no licence to carry, and the volume is the
 * reader's own setting. Each sound is a plan first — `completionNotes` — and
 * played second — `playCompletionSound` — so the plan can be checked on a
 * machine with no audio device.
 */

/** The built-in sounds, by the name the settings file stores. */
export type CompletionSound = "chime" | "ping" | "woodblock";

/** One tone: a frequency, when it starts, how long it lasts, and how loud. */
export interface CompletionNote {
  frequency: number;
  /** Seconds from the start of the sound. */
  start: number;
  duration: number;
  /** The peak gain before the reader's volume is applied. */
  peak: number;
  wave: OscillatorType;
}

/** The sounds the settings page offers, in the order it lists them. */
export const COMPLETION_SOUNDS: { value: CompletionSound; label: string }[] = [
  { value: "chime", label: "Chime" },
  { value: "ping", label: "Ping" },
  { value: "woodblock", label: "Woodblock" },
];

/** The default, and the fallback for a name that is not one of the three. */
export const DEFAULT_COMPLETION_SOUND: CompletionSound = "chime";

const NOTES: Record<CompletionSound, readonly CompletionNote[]> = {
  // Two falling bells: what a finished transfer has sounded like for decades.
  chime: [
    { frequency: 1046.5, start: 0, duration: 0.42, peak: 0.2, wave: "sine" },
    { frequency: 784, start: 0.13, duration: 0.5, peak: 0.16, wave: "sine" },
  ],
  // One short, bright note: quiet enough for a queue that finishes in bursts.
  ping: [{ frequency: 1568, start: 0, duration: 0.22, peak: 0.15, wave: "sine" }],
  // Two dry knocks: for a reader who wants to be told, not played to.
  woodblock: [
    { frequency: 990, start: 0, duration: 0.07, peak: 0.18, wave: "triangle" },
    { frequency: 660, start: 0.09, duration: 0.09, peak: 0.15, wave: "triangle" },
  ],
};

export function isCompletionSound(value: unknown): value is CompletionSound {
  return COMPLETION_SOUNDS.some((sound) => sound.value === value);
}

/** The notes of one sound; an unknown name plays the default. */
export function completionNotes(name: unknown): readonly CompletionNote[] {
  return NOTES[isCompletionSound(name) ? name : DEFAULT_COMPLETION_SOUND];
}

/** The setting as a fraction of full scale, 0 to 1. */
export function completionLevel(volume: unknown): number {
  const value =
    typeof volume === "number" && Number.isFinite(volume) ? volume : 0;
  return Math.min(100, Math.max(0, Math.round(value))) / 100;
}

let device: AudioContext | undefined;

/** The audio device, made once and reused. undefined when it cannot be had. */
function audioDevice(): AudioContext | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }
  const host = window as Window & {
    webkitAudioContext?: typeof AudioContext;
  };
  const Device = window.AudioContext ?? host.webkitAudioContext;
  if (!Device) {
    return undefined;
  }
  try {
    device ??= new Device();
  } catch {
    return undefined;
  }
  // An audio device starts suspended until the page has been touched once.
  if (device.state === "suspended") {
    void device.resume();
  }
  return device;
}

/**
 * Play one sound at the reader's volume.
 *
 * True when it was played. False is not an error: zero volume, or a host with
 * no audio device, is a finished download nobody asked to hear.
 */
export function playCompletionSound(name: unknown, volume: unknown): boolean {
  const level = completionLevel(volume);
  if (level <= 0) {
    return false;
  }
  const context = audioDevice();
  if (!context) {
    return false;
  }
  const start = context.currentTime + 0.01;
  for (const note of completionNotes(name)) {
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.type = note.wave;
    oscillator.frequency.value = note.frequency;
    const begin = start + note.start;
    const end = begin + note.duration;
    // An exponential ramp cannot start or end at zero, so both ends are the
    // quietest gain the curve accepts rather than silence.
    gain.gain.setValueAtTime(0.0001, begin);
    gain.gain.exponentialRampToValueAtTime(note.peak * level, begin + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, end);
    oscillator.connect(gain);
    gain.connect(context.destination);
    oscillator.start(begin);
    oscillator.stop(end + 0.02);
  }
  return true;
}
