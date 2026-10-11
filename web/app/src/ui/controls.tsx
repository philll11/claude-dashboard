// The dashboard's own controls, drawn with the design's tokens so that they follow the desktop's
// theme and VS Code's alike.

import {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useEffectEvent,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { Check, ChevronDown, Search } from "./icons";

/** Text for screen readers only. */
export function Hidden({ children }: { children: ReactNode }) {
  return <span className="visually-hidden">{children}</span>;
}

export interface Choice<T> {
  value: T;
  label: string;
}

/** A row of buttons of which one is pressed, such as the date range. */
export function Segmented<T extends string>({
  label,
  value,
  choices,
  onChange,
}: {
  label: string;
  value: T;
  choices: readonly Choice<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="group" aria-label={label} className="segmented">
      {choices.map((choice) => (
        <button
          key={choice.value}
          type="button"
          aria-pressed={choice.value === value}
          onClick={() => {
            onChange(choice.value);
          }}
        >
          {choice.label}
        </button>
      ))}
    </div>
  );
}

/** A button that opens a menu of choices, of which one is checked: a filter. */
export function Picker<T>({
  label,
  value,
  choices,
  onChange,
  mono = false,
}: {
  /** What the choice is of, such as `Project`. */
  label: string;
  value: T;
  choices: readonly Choice<T>[];
  onChange: (value: T) => void;
  /** Shows the choices in the monospace font, as names of folders are. */
  mono?: boolean;
}) {
  const [open, setOpen] = useState<"first" | "last" | "checked" | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const checked = Math.max(
    0,
    choices.findIndex((choice) => Object.is(choice.value, value)),
  );
  const current = choices[checked];

  const items = () => [
    ...(menu.current?.querySelectorAll<HTMLElement>("[role=menuitemradio]") ?? []),
  ];

  useLayoutEffect(() => {
    if (open === null) return;
    const all = items();
    const target = open === "first" ? all[0] : open === "last" ? all.at(-1) : all[checked];
    target?.focus();
    target?.scrollIntoView({ block: "nearest" });
  }, [open, checked]);

  useEffect(() => {
    if (open === null) return;
    const close = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !button.current?.contains(target)) setOpen(null);
    };
    document.addEventListener("pointerdown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
    };
  }, [open]);

  const choose = (choice: Choice<T>) => {
    setOpen(null);
    button.current?.focus();
    if (!Object.is(choice.value, value)) onChange(choice.value);
  };

  const onButtonKey = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setOpen("checked");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setOpen("last");
    }
  };

  const onMenuKey = (event: KeyboardEvent) => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const focus = (index: number) => all[(index + all.length) % all.length]?.focus();
    switch (event.key) {
      case "ArrowDown":
        focus(at + 1);
        break;
      case "ArrowUp":
        focus(at - 1);
        break;
      case "Home":
        focus(0);
        break;
      case "End":
        focus(all.length - 1);
        break;
      case "Escape":
        setOpen(null);
        button.current?.focus();
        break;
      case "Tab":
        setOpen(null);
        return;
      default: {
        if (event.key.length !== 1) return;
        const typed = event.key.toLowerCase();
        const next = [...all.slice(at + 1), ...all.slice(0, at + 1)].find((item) =>
          item.textContent.trim().toLowerCase().startsWith(typed),
        );
        next?.focus();
      }
    }
    event.preventDefault();
  };

  return (
    <div className="picker">
      <button
        ref={button}
        type="button"
        className="button"
        aria-haspopup="menu"
        aria-expanded={open !== null}
        aria-controls={open === null ? undefined : menuId}
        onClick={() => {
          setOpen(open === null ? "checked" : null);
        }}
        onKeyDown={onButtonKey}
      >
        <Hidden>{label}: </Hidden>
        <span className={mono ? "mono" : undefined}>{current?.label}</span>
        <ChevronDown />
      </button>
      {open !== null && (
        <div
          ref={menu}
          id={menuId}
          role="menu"
          aria-label={label}
          className="menu"
          onKeyDown={onMenuKey}
        >
          {choices.map((choice, index) => (
            <button
              key={choice.label}
              type="button"
              role="menuitemradio"
              aria-checked={index === checked}
              tabIndex={-1}
              className={mono && index > 0 ? "mono" : undefined}
              onClick={() => {
                choose(choice);
              }}
            >
              <span className="check">{index === checked && <Check />}</span>
              {choice.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** A search box that reports what is typed once typing pauses. */
export function SearchField({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const [text, setText] = useState(value);
  const [given, setGiven] = useState(value);
  if (value !== given) {
    setGiven(value);
    setText(value);
  }
  const report = useEffectEvent((typed: string) => {
    onChange(typed);
  });
  useEffect(() => {
    if (text === value) return;
    const timer = setTimeout(() => {
      report(text);
    }, 250);
    return () => {
      clearTimeout(timer);
    };
  }, [text, value]);
  return (
    <label className="search">
      <Search />
      <Hidden>{label}</Hidden>
      <input
        type="search"
        placeholder={placeholder}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
        }}
      />
    </label>
  );
}

/** An on and off setting. */
export function Switch({
  checked,
  onChange,
  labelledBy,
  describedBy,
  disabled = false,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  labelledBy: string;
  describedBy?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      disabled={disabled}
      onClick={() => {
        onChange(!checked);
      }}
    >
      <span className="switch-thumb" />
    </button>
  );
}
