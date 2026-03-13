export class DoBase<T = unknown> {
  get(): Promise<T> { return Promise.resolve({} as T); }
  set(_state: T): Promise<void> { return Promise.resolve(); }
}
