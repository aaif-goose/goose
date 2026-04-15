import { invoke } from "@tauri-apps/api/core";

export type FixType = "command" | "bridge";

export interface DoctorCheck {
  id: string;
  label: string;
  status: "pass" | "warn" | "fail";
  message: string;
  fixUrl: string | null;
  fixCommand: string | null;
  fixType: FixType | null;
  path: string | null;
  bridgePath: string | null;
  rawOutput: string | null;
}

export interface DoctorReport {
  checks: DoctorCheck[];
}

export async function runDoctor(): Promise<DoctorReport> {
  return invoke("run_doctor");
}

export async function runDoctorFix(
  checkId: string,
  fixType: FixType,
): Promise<void> {
  return invoke("run_doctor_fix", { checkId, fixType });
}
