export interface TerminalOptions {
  serverUrl?: string;
  fontSize?: number;
}

interface WasmModule {
  default: () => Promise<unknown>;
  create_terminal: (
    containerId: string,
    wsUrl: string,
    fontSize: number,
  ) => void;
}

/**
 * Embeddable WebGPU terminal component.
 *
 * @example
 * ```ts
 * const terminal = await Terminal.init(
 *   document.getElementById("terminal")!,
 *   { serverUrl: "wss://example.com/ws" },
 * );
 * ```
 */
export class Terminal {
  private containerId: string;

  private constructor(containerId: string) {
    this.containerId = containerId;
  }

  /**
   * Initialize a terminal inside the given container element.
   *
   * @param container - DOM element to mount the terminal into
   * @param options - Configuration options
   * @returns Initialized terminal instance
   */
  static async init(
    container: HTMLElement,
    options: TerminalOptions = {},
  ): Promise<Terminal> {
    const id = container.id || `omni-terminal-${Date.now()}`;
    container.id = id;

    // Resolve WASM glue relative to this module (co-located in build/)
    const wasmPath = "./omni_terminal_wasm.js";
    const wasmModule = (await import(wasmPath)) as WasmModule;
    await wasmModule.default();

    const serverUrl =
      options.serverUrl ??
      `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`;
    const fontSize = options.fontSize ?? 16;

    wasmModule.create_terminal(id, serverUrl, fontSize);

    return new Terminal(id);
  }

  /** Remove the terminal from the DOM */
  dispose(): void {
    const el = document.getElementById(this.containerId);
    if (el) el.innerHTML = "";
  }
}
