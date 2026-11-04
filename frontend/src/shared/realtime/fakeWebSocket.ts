// Minimal WebSocket stand-in we can drive synchronously from tests.
export class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  url: string;
  protocols: string | string[] | undefined;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(url: string, protocols?: string | string[]) {
    this.url = url;
    this.protocols = protocols;
    FakeWebSocket.instances.push(this);
  }
  // test drivers
  open() {
    this.onopen?.();
  }
  emit(event: unknown) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
  emitRaw(data: string) {
    this.onmessage?.({ data });
  }
  serverClose() {
    this.onclose?.();
  }
  close() {
    this.closed = true;
  }
}

export const WS = FakeWebSocket as unknown as typeof WebSocket;
export const latest = () => FakeWebSocket.instances.at(-1)!;
