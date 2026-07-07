/**
 * @clotho/sdk-js — typed client for the Clotho API gateway.
 *
 * This will be generated from the gateway's OpenAPI spec once
 * clotho-api-gateway exposes real routes (docs/prd.md §2). Until then it's a
 * hand-written stub matching the health endpoint the gateway will proxy.
 */

export interface HealthStatus {
  status: "serving" | "not_serving";
  serviceName: string;
  version: string;
}

export interface ClothoClientOptions {
  /** Base URL of the Clotho API gateway, e.g. http://localhost:50056 */
  baseUrl: string;
  fetch?: typeof fetch;
}

export class ClothoClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: ClothoClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.fetchImpl = options.fetch ?? fetch;
  }

  async health(): Promise<HealthStatus> {
    const res = await this.fetchImpl(`${this.baseUrl}/v1/health`);
    if (!res.ok) {
      throw new Error(`health check failed: ${res.status}`);
    }
    return (await res.json()) as HealthStatus;
  }
}
