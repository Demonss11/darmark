// Типы палитры команд.

export interface PaletteCommand {
  id: string;
  title: string;
  group: string;
  origin: "core" | `plugin:${string}`;
  hotkey?: string;
  run(): void | Promise<void>;
}

export interface CommandProvider {
  list(): PaletteCommand[] | Promise<PaletteCommand[]>;
}

export interface PaletteOptions {
  providers: CommandProvider[];
  onError?(message: string): void;
}

export interface PaletteController {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
}
