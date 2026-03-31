import * as tauri from "../tauri";

type LogLevel = "debug" | "info" | "warn" | "error";

/**
 * Logger that writes to ~/Library/Logs/Noema/noema.log
 * Works in both dev mode and production builds.
 */
class Logger {
  private source: string;

  constructor(source: string) {
    this.source = source;
  }

  private log(level: LogLevel, message: string, data?: unknown) {
    const formatted = data !== undefined
      ? `${message} ${JSON.stringify(data)}`
      : message;

    // Also log to console for dev convenience
    const consoleMethod = level === "error" ? console.error
      : level === "warn" ? console.warn
      : level === "debug" ? console.debug
      : console.log;
    consoleMethod(`[${this.source}]`, message, data ?? "");

    // Send to backend to write to log file
    tauri.logDebug(level, this.source, formatted).catch(() => {
      // Ignore errors - logging shouldn't break the app
    });
  }

  debug(message: string, data?: unknown) {
    this.log("debug", message, data);
  }

  info(message: string, data?: unknown) {
    this.log("info", message, data);
  }

  warn(message: string, data?: unknown) {
    this.log("warn", message, data);
  }

  error(message: string, data?: unknown) {
    this.log("error", message, data);
  }
}

/**
 * Create a logger for a specific component/module
 * @param source - Name of the component (e.g., "AudioPlayer", "VoiceInput")
 */
export function createLogger(source: string): Logger {
  return new Logger(source);
}

// Pre-created loggers for common modules
export const appLog = createLogger("App");
export const voiceLog = createLogger("Voice");
export const audioLog = createLogger("Audio");
export const mcpLog = createLogger("MCP");
