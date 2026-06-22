import assert from "node:assert/strict";
import {
  CronExpression,
  CronExpressionError,
  nextCronDueAt,
} from "./cron-expression.js";

const weekdayMorning = new CronExpression("*/15 9-10 * * 1-5");
assert.equal(
  weekdayMorning.nextAfter(new Date("2026-06-15T09:01:10Z")).toISOString(),
  "2026-06-15T09:15:00.000Z",
);
assert.equal(
  weekdayMorning.nextAfter(new Date("2026-06-15T10:45:00Z")).toISOString(),
  "2026-06-16T09:00:00.000Z",
);

const listsAndSteps = new CronExpression("5,10 0,12 1-3/2 6 1-5");
assert.equal(listsAndSteps.matches(new Date("2026-06-01T12:05:00Z")), true);
assert.equal(listsAndSteps.matches(new Date("2026-06-02T12:05:00Z")), false);
assert.equal(listsAndSteps.matches(new Date("2026-06-06T12:05:00Z")), false);
assert.equal(
  nextCronDueAt("0 0 1 1 *", "2026-06-21T00:00:00Z"),
  "2027-01-01T00:00:00.000Z",
);

assert.throws(() => new CronExpression("* * *"), CronExpressionError);
assert.throws(() => new CronExpression("61 * * * *"), CronExpressionError);
assert.throws(() => new CronExpression("*/0 * * * *"), CronExpressionError);
assert.throws(() => new CronExpression("10-5 * * * *"), CronExpressionError);
assert.throws(() => new CronExpression("*//2 * * * *"), CronExpressionError);

console.log(
  JSON.stringify(
    {
      weekdayNext: weekdayMorning
        .nextAfter(new Date("2026-06-15T09:01:10Z"))
        .toISOString(),
      yearlyNext: nextCronDueAt("0 0 1 1 *", "2026-06-21T00:00:00Z"),
    },
    null,
    2,
  ),
);
