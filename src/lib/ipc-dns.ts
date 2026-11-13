/**
 * The DNS the engine asks names with.
 *
 * The engine owns the choice: it is stored in the engine's settings file and
 * put in force for every client — downloads, the grabber, video tracks, the
 * update check.
 */

import { invoke } from "@/lib/ipc";

/**
 * The DNS the engine asks names with.
 *
 * One resolver governs the whole application: every client built after the
 * setting is applied looks names up through it.
 */
export interface DnsSettings {
  /** "system", "doh", "dot" or "nextdns". */
  mode: DnsMode;
  /** The DNS over HTTPS endpoint, for the "doh" mode. */
  dohUrl: string;
  /** The hostname asked over TLS, for the "dot" mode. */
  dotHost: string;
  /** The NextDNS configuration id, for the "nextdns" mode. */
  nextdnsProfile: string;
  /** Whether the system's resolver may answer when the chosen one cannot. */
  fallback: boolean;
}

/** The ways names can be resolved. */
export type DnsMode = "system" | "doh" | "dot" | "nextdns";

/**
 * Store the DNS the engine asks with, and put it in force.
 *
 * The engine checks the values before it keeps them, so an endpoint that
 * cannot be asked is refused here rather than failed at the first download.
 */
export function setDns(dns: DnsSettings): Promise<DnsSettings> {
  return invoke<DnsSettings>("set_dns", { dns });
}

/** The DNS in force, as the engine has it. */
export function dnsSettings(): Promise<DnsSettings> {
  return invoke<DnsSettings>("dns_settings");
}

/** What the test button found. */
export interface DnsTest {
  /** How long the lookup took, in milliseconds. */
  milliseconds: number;
  /** The addresses the resolver answered with. */
  addresses: string[];
}

/**
 * Ask the resolver the reader chose for one name.
 *
 * The chosen resolver answers or it does not; the system's may not answer
 * instead, so a hidden fallback cannot make a dead resolver look alive.
 */
export function testDns(dns: DnsSettings): Promise<DnsTest> {
  return invoke<DnsTest>("test_dns", { dns });
}
