import { InputBuffer } from "./input-buffer.js";

export enum InteractionPhase {
  Idle = "idle",
  Streaming = "streaming",
  Approval = "approval",
  Normal = "normal",
}

export type InputReleaseHandler = (entries: string[]) => void;
export type PhaseChangeHandler = (from: InteractionPhase, to: InteractionPhase, data?: string) => void;

const TRANSITIONS: Record<InteractionPhase, InteractionPhase[]> = {
  [InteractionPhase.Idle]: [InteractionPhase.Streaming, InteractionPhase.Normal],
  [InteractionPhase.Streaming]: [InteractionPhase.Idle, InteractionPhase.Approval, InteractionPhase.Normal],
  [InteractionPhase.Approval]: [InteractionPhase.Idle, InteractionPhase.Streaming],
  [InteractionPhase.Normal]: [InteractionPhase.Idle, InteractionPhase.Streaming],
};

export class PhaseManager {
  private _phase: InteractionPhase = InteractionPhase.Idle;
  readonly inputBuffer = new InputBuffer();
  private _onPhaseChange?: PhaseChangeHandler;
  private _onInputRelease?: InputReleaseHandler;

  get phase(): InteractionPhase {
    return this._phase;
  }

  get hasBufferedInput(): boolean {
    return this.inputBuffer.length > 0;
  }

  set onPhaseChange(handler: PhaseChangeHandler | undefined) {
    this._onPhaseChange = handler;
  }

  set onInputRelease(handler: InputReleaseHandler | undefined) {
    this._onInputRelease = handler;
  }

  transition(to: InteractionPhase, data?: string): boolean {
    const from = this._phase;
    if (from === to) return false;
    if (!this.isValidTransition(from, to)) return false;

    if (from === InteractionPhase.Streaming && to !== InteractionPhase.Streaming) {
      this.releaseBufferedInput();
    }

    this._phase = to;
    this._onPhaseChange?.(from, to, data);
    return true;
  }

  handleInput(data: string): { consumed: boolean; buffered: boolean } {
    if (this._phase === InteractionPhase.Streaming) {
      if (data.length === 1) {
        const code = data.charCodeAt(0);
        if (code >= 0x20 && code !== 0x7f) {
          this.inputBuffer.push(data);
          return { consumed: true, buffered: true };
        }
      }
      return { consumed: false, buffered: false };
    }
    return { consumed: false, buffered: false };
  }

  releaseBufferedInput(): void {
    const entries = this.inputBuffer.flush();
    if (entries.length === 0) return;
    this._onInputRelease?.(entries);
  }

  clearBufferedInput(): void {
    this.inputBuffer.clear();
  }

  private isValidTransition(from: InteractionPhase, to: InteractionPhase): boolean {
    return TRANSITIONS[from]?.includes(to) ?? false;
  }

  static getValidTransitions(from: InteractionPhase): InteractionPhase[] {
    return [...(TRANSITIONS[from] ?? [])];
  }
}
