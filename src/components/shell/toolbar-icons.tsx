export interface ToolbarIconProps {
  className?: string;
}

interface GlossProps {
  id: string;
  from: string;
  to: string;
}

/** A two-stop top-to-bottom highlight shared by the toolbar artwork. */
function Gloss({ id, from, to }: GlossProps) {
  return (
    <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" style={{ stopColor: `var(${from})` }} />
      <stop offset="1" style={{ stopColor: `var(${to})` }} />
    </linearGradient>
  );
}

export function AddIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-add" from="--icon-green-bright" to="--icon-green" />
      </defs>
      <path
        d="M14 4h4a2 2 0 0 1 2 2v20a2 2 0 0 1-2 2h-4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z"
        fill="url(#grad-add)"
        stroke="var(--icon-green-deep)"
        strokeOpacity=".45"
        strokeWidth=".8"
      />
      <path
        d="M6 12a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2Z"
        fill="url(#grad-add)"
        stroke="var(--icon-green-deep)"
        strokeOpacity=".45"
        strokeWidth=".8"
      />
      <path d="M14.2 5.5h1.4v10h-1.4z" fill="#fff" opacity=".25" />
      <path d="M7.5 12.2h10v1.4h-10z" fill="#fff" opacity=".25" />
    </svg>
  );
}

export function ResumeIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-resume-ring" from="--icon-silver-bright" to="--icon-silver" />
        <Gloss id="grad-resume-arrow" from="--icon-green-bright" to="--icon-green" />
      </defs>
      <circle
        cx="16"
        cy="17"
        r="11.8"
        fill="none"
        stroke="url(#grad-resume-ring)"
        strokeWidth="3.4"
      />
      <path
        d="M16 8.5v9.8"
        stroke="url(#grad-resume-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
      />
      <path
        d="m10.8 13.9 5.2 5.2 5.2-5.2"
        fill="none"
        stroke="url(#grad-resume-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M8.6 12.2A9.6 9.6 0 0 1 15 6.4"
        fill="none"
        stroke="#fff"
        strokeOpacity=".45"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function StopIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-stop" from="--icon-red-bright" to="--icon-red" />
      </defs>
      <rect
        x="4.5"
        y="4.5"
        width="23"
        height="23"
        rx="4"
        fill="url(#grad-stop)"
        stroke="var(--icon-red-deep)"
        strokeOpacity=".55"
        strokeWidth="1"
      />
      <rect x="11.5" y="11.5" width="9" height="9" rx="1" fill="#fff" opacity=".92" />
      <path
        d="M9 5.7h14"
        stroke="#fff"
        strokeOpacity=".28"
        strokeWidth="1.2"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function StopAllIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-stopall" from="--icon-silver-bright" to="--icon-silver" />
      </defs>
      <g
        fill="url(#grad-stopall)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".5"
        strokeWidth=".9"
      >
        <rect x="10" y="6.5" width="4" height="13" rx="2" />
        <rect x="14.6" y="4.5" width="4" height="15" rx="2" />
        <rect x="19.2" y="6" width="4" height="13.5" rx="2" />
        <rect x="23.6" y="8.5" width="3.6" height="11" rx="1.8" />
        <rect
          x="4.6"
          y="16"
          width="8.2"
          height="3.9"
          rx="1.95"
          transform="rotate(28 8.7 18)"
        />
        <path d="M9.2 14.2h18.4v7.9c0 4.3-3.3 7.5-7.7 7.5h-4.3c-2.7 0-5.2-1.5-6.5-3.9l-2.6-4.8a2 2 0 0 1 3.4-2.1l1.4 1.6z" />
      </g>
      <path
        d="M12 8v4M21.2 8v3"
        stroke="#fff"
        strokeOpacity=".35"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function DeleteIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-delete" from="--icon-silver-bright" to="--icon-silver" />
      </defs>
      <path
        d="M12.4 9.3V7a2 2 0 0 1 2-2h3.2a2 2 0 0 1 2 2v2.3"
        fill="none"
        stroke="var(--icon-silver)"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
      <path
        d="M6.8 9.5h18.4"
        stroke="var(--icon-silver)"
        strokeWidth="3"
        strokeLinecap="round"
      />
      <path
        d="M9.4 11.5h13.2l-1.1 13.9a2 2 0 0 1-2 1.9h-7a2 2 0 0 1-2-1.9z"
        fill="url(#grad-delete)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".45"
        strokeWidth=".9"
      />
      <path
        d="M13.4 15v8M18.6 15v8"
        stroke="var(--icon-silver-deep)"
        strokeWidth="1.4"
        strokeLinecap="round"
        opacity=".75"
      />
      <path
        d="M8.2 9.5h15.6"
        stroke="#fff"
        strokeOpacity=".3"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function DeleteCompletedIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-delete-done" from="--icon-silver-bright" to="--icon-silver" />
        <Gloss id="grad-delete-done-arrow" from="--icon-green-bright" to="--icon-green" />
      </defs>
      <path
        d="M9.6 11.2V9.4a1.8 1.8 0 0 1 1.8-1.8h2.4a1.8 1.8 0 0 1 1.8 1.8v1.8"
        fill="none"
        stroke="var(--icon-silver)"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
      <path
        d="M5.4 11.4h14.6"
        stroke="var(--icon-silver)"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
      <path
        d="M7.6 13.2h10.4l-.9 11.9a1.8 1.8 0 0 1-1.8 1.7h-5.9a1.8 1.8 0 0 1-1.8-1.7z"
        fill="url(#grad-delete-done)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".45"
        strokeWidth=".8"
      />
      <path
        d="M10.7 16.2v6.6M14.9 16.2v6.6"
        stroke="var(--icon-silver-deep)"
        strokeWidth="1.2"
        strokeLinecap="round"
        opacity=".7"
      />
      <path
        d="M23.4 8.2v10.6"
        stroke="var(--chrome-toolbar)"
        strokeWidth="6.4"
        strokeLinecap="round"
      />
      <path
        d="m18.8 15.4 4.6 4.6 4.6-4.6"
        fill="none"
        stroke="var(--chrome-toolbar)"
        strokeWidth="6.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M23.4 8.2v10.2"
        stroke="url(#grad-delete-done-arrow)"
        strokeWidth="3.8"
        strokeLinecap="round"
      />
      <path
        d="m19.2 15 4.2 4.2 4.2-4.2"
        fill="none"
        stroke="url(#grad-delete-done-arrow)"
        strokeWidth="3.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function OptionsIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-options" from="--icon-purple-bright" to="--icon-purple" />
      </defs>
      <g
        fill="url(#grad-options)"
        stroke="var(--icon-purple-deep)"
        strokeOpacity=".4"
        strokeWidth=".7"
      >
        {[0, 45, 90, 135].map((angle) => (
          <rect
            key={angle}
            x="14"
            y="2.5"
            width="4"
            height="8"
            rx="1.3"
            transform={`rotate(${angle} 16 16)`}
          />
        ))}
        <circle cx="16" cy="16" r="9.4" />
      </g>
      <circle cx="16" cy="16" r="3.9" fill="var(--icon-purple-deep)" />
      <path
        d="M11.4 9.4A8.6 8.6 0 0 1 19 6.7"
        fill="none"
        stroke="#fff"
        strokeOpacity=".3"
        strokeWidth="1.1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function SchedulerIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-clock" from="--icon-silver-bright" to="--icon-silver" />
      </defs>
      <rect x="13.4" y="2.4" width="5.2" height="3.2" rx="1.2" fill="var(--icon-silver)" />
      <circle
        cx="16"
        cy="17.4"
        r="11.6"
        fill="url(#grad-clock)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".65"
        strokeWidth="1"
      />
      <circle cx="16" cy="17.4" r="8.6" fill="var(--icon-silver-bright)" />
      <path
        d="M16 9.2v1.4M16 24.2v1.4M8.8 17.4h1.4M22.2 17.4h1.4"
        stroke="var(--icon-silver-deep)"
        strokeWidth="1"
        strokeLinecap="round"
      />
      <path
        d="M16 17.4v-5.2M16 17.4l4.2 2.8"
        stroke="var(--chrome-toolbar)"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M16 17.4v-4"
        stroke="var(--icon-red)"
        strokeWidth="1"
        strokeLinecap="round"
      />
      <circle cx="16" cy="17.4" r="1.2" fill="var(--chrome-toolbar)" />
      <path
        d="M8 12.6a9.2 9.2 0 0 1 4.8-4.4"
        fill="none"
        stroke="#fff"
        strokeOpacity=".45"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function StartQueueIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-startqueue-tray" from="--icon-silver-bright" to="--icon-silver" />
        <Gloss id="grad-startqueue-arrow" from="--icon-green-bright" to="--icon-green" />
      </defs>
      <path
        d="M16 4v10.6"
        stroke="url(#grad-startqueue-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
      />
      <path
        d="m10.6 9.9 5.4 5.4 5.4-5.4"
        fill="none"
        stroke="url(#grad-startqueue-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M4.4 18.2h6.2l1.7 3.2h7.4l1.7-3.2h6.2v7.2a3.4 3.4 0 0 1-3.4 3.4H7.8a3.4 3.4 0 0 1-3.4-3.4z"
        fill="url(#grad-startqueue-tray)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".55"
        strokeWidth="1"
      />
      <path
        d="M5.4 18.2h21.2"
        stroke="#fff"
        strokeOpacity=".4"
        strokeWidth="1"
        strokeLinecap="round"
      />
      <path
        d="M7.4 25.2h17.2"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".4"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function StopQueueIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-stopqueue-tray" from="--icon-silver-bright" to="--icon-silver" />
        <Gloss id="grad-stopqueue-arrow" from="--icon-amber-bright" to="--icon-amber" />
      </defs>
      <path
        d="M16 17.4V5.6"
        stroke="url(#grad-stopqueue-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
      />
      <path
        d="m10.6 11 5.4-5.4L21.4 11"
        fill="none"
        stroke="url(#grad-stopqueue-arrow)"
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M4.4 18.2h6.2l1.7 3.2h7.4l1.7-3.2h6.2v7.2a3.4 3.4 0 0 1-3.4 3.4H7.8a3.4 3.4 0 0 1-3.4-3.4z"
        fill="url(#grad-stopqueue-tray)"
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".55"
        strokeWidth="1"
      />
      <path
        d="M5.4 18.2h21.2"
        stroke="#fff"
        strokeOpacity=".4"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function TellFriendIcon({ className }: ToolbarIconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id="grad-friend" from="--icon-blue-bright" to="--icon-blue" />
      </defs>
      <g transform="translate(4 4)">
        <path
          d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z"
          fill="url(#grad-friend)"
          stroke="var(--icon-blue-deep)"
          strokeOpacity=".5"
          strokeWidth=".8"
        />
        <path
          d="M7 10v12"
          stroke="var(--icon-blue-deep)"
          strokeWidth="1.3"
          strokeLinecap="round"
          opacity=".8"
        />
        <path
          d="M11.5 7.2a2.4 2.4 0 0 1 2-2.2"
          fill="none"
          stroke="#fff"
          strokeOpacity=".4"
          strokeWidth="1"
          strokeLinecap="round"
        />
      </g>
    </svg>
  );
}
