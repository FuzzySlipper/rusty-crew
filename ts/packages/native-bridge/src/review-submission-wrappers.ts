import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type ReviewSubmissionMethodName =
  | "beginReviewSubmission"
  | "transitionReviewSubmission"
  | "listReviewSubmissions";

export function createNativeBridgeReviewSubmissionMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ReviewSubmissionMethodName> {
  return {
    beginReviewSubmission: async (input) =>
      JSON.parse(
        binding.beginReviewSubmissionJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["beginReviewSubmission"]>>,
    transitionReviewSubmission: async (input) =>
      JSON.parse(
        binding.transitionReviewSubmissionJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["transitionReviewSubmission"]>
      >,
    listReviewSubmissions: async (query = { pendingOnly: false }) =>
      JSON.parse(
        binding.listReviewSubmissionsJson(JSON.stringify(query)),
      ) as Awaited<ReturnType<NativeBridgeModule["listReviewSubmissions"]>>,
  };
}
