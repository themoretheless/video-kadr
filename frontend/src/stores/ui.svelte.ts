class UiStore {
  #busyScopes = $state<string[]>([])
  #message = $state('')

  get busy(): boolean {
    return this.#busyScopes.length > 0
  }

  get message(): string {
    return this.#message
  }

  begin(scope: string): void {
    if (!this.#busyScopes.includes(scope)) this.#busyScopes = [...this.#busyScopes, scope]
  }

  finish(scope: string): void {
    this.#busyScopes = this.#busyScopes.filter((candidate) => candidate !== scope)
  }

  report(message: string): void {
    this.#message = message
  }
}

export const uiStore = new UiStore()
