/** Fail-closed UTC cron expression parser for minute-granularity schedules. */

export interface CronFieldRange {
  readonly min: number;
  readonly max: number;
}

export interface CronNextOptions {
  maxSearchMinutes?: number;
}

const fieldRanges: readonly CronFieldRange[] = [
  { min: 0, max: 59 },
  { min: 0, max: 23 },
  { min: 1, max: 31 },
  { min: 1, max: 12 },
  { min: 0, max: 6 },
];

export class CronExpressionError extends Error {
  readonly code = "CRON_EXPRESSION_ERROR";

  constructor(message: string) {
    super(message);
    this.name = "CronExpressionError";
  }
}

export class CronExpression {
  readonly #fields: readonly ReadonlySet<number>[];

  constructor(readonly expression: string) {
    const fields = expression.trim().split(/\s+/);
    if (fields.length !== 5) {
      throw new CronExpressionError(
        "cron schedule must contain exactly 5 fields",
      );
    }
    this.#fields = fields.map((field, index) =>
      parseField(field, requireRange(index)),
    );
  }

  nextAfter(date: Date, options: CronNextOptions = {}): Date {
    if (Number.isNaN(date.getTime())) {
      throw new CronExpressionError("cron nextAfter requires a valid date");
    }
    const cursor = new Date(date.getTime());
    cursor.setUTCSeconds(0, 0);
    cursor.setUTCMinutes(cursor.getUTCMinutes() + 1);
    const maxSearchMinutes = options.maxSearchMinutes ?? 366 * 24 * 60;
    const deadline = cursor.getTime() + maxSearchMinutes * 60 * 1000;
    while (cursor.getTime() <= deadline) {
      if (this.matches(cursor)) return new Date(cursor.getTime());
      cursor.setUTCMinutes(cursor.getUTCMinutes() + 1);
    }
    throw new CronExpressionError("cron schedule has no run within one year");
  }

  matches(date: Date): boolean {
    if (Number.isNaN(date.getTime())) return false;
    const values = [
      date.getUTCMinutes(),
      date.getUTCHours(),
      date.getUTCDate(),
      date.getUTCMonth() + 1,
      date.getUTCDay(),
    ];
    return values.every(
      (value, index) => this.#fields[index]?.has(value) ?? false,
    );
  }
}

export function nextCronDueAt(
  expression: string,
  after: string | Date,
): string {
  const date = typeof after === "string" ? new Date(after) : after;
  return new CronExpression(expression).nextAfter(date).toISOString();
}

function requireRange(index: number): CronFieldRange {
  const range = fieldRanges[index];
  if (range === undefined) {
    throw new CronExpressionError("missing cron field range");
  }
  return range;
}

function parseField(raw: string, range: CronFieldRange): ReadonlySet<number> {
  const values = new Set<number>();
  for (const part of raw.split(",")) {
    addPart(values, part, range);
  }
  if (values.size === 0) {
    throw new CronExpressionError(`empty cron field: ${raw}`);
  }
  return values;
}

function addPart(
  values: Set<number>,
  raw: string,
  range: CronFieldRange,
): void {
  const [base, stepRaw, extra] = raw.split("/");
  if (extra !== undefined) {
    throw new CronExpressionError(`invalid cron step expression: ${raw}`);
  }
  const step = stepRaw === undefined ? 1 : parsePositiveInt(stepRaw, "step");
  const [start, end] = parseBase(base ?? "", range);
  for (let value = start; value <= end; value += step) {
    values.add(value);
  }
}

function parseBase(
  raw: string,
  range: CronFieldRange,
): readonly [number, number] {
  if (raw === "*") return [range.min, range.max];
  const dash = raw.indexOf("-");
  if (dash >= 0) {
    const start = parseCronValue(raw.slice(0, dash), range);
    const end = parseCronValue(raw.slice(dash + 1), range);
    if (start > end) {
      throw new CronExpressionError(`invalid descending range: ${raw}`);
    }
    return [start, end];
  }
  const value = parseCronValue(raw, range);
  return [value, value];
}

function parseCronValue(raw: string, range: CronFieldRange): number {
  const value = parsePositiveOrZeroInt(raw, "field");
  if (value < range.min || value > range.max) {
    throw new CronExpressionError(`cron value out of range: ${raw}`);
  }
  return value;
}

function parsePositiveInt(raw: string, label: string): number {
  const value = parsePositiveOrZeroInt(raw, label);
  if (value <= 0) {
    throw new CronExpressionError(`cron ${label} must be positive: ${raw}`);
  }
  return value;
}

function parsePositiveOrZeroInt(raw: string, label: string): number {
  if (!/^\d+$/.test(raw)) {
    throw new CronExpressionError(`invalid cron ${label}: ${raw}`);
  }
  return Number(raw);
}
