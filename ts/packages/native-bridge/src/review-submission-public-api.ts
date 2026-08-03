import type {
  ReviewSubmissionQuery,
  ReviewSubmissionRecord,
  ReviewSubmissionRequest,
  ReviewSubmissionTransitionRequest,
} from "@rusty-crew/contracts";

export interface NativeReviewSubmissionBridgeMethods {
  beginReviewSubmission(
    input: ReviewSubmissionRequest,
  ): Promise<ReviewSubmissionRecord>;
  transitionReviewSubmission(
    input: ReviewSubmissionTransitionRequest,
  ): Promise<ReviewSubmissionRecord>;
  listReviewSubmissions(
    query?: ReviewSubmissionQuery,
  ): Promise<ReviewSubmissionRecord[]>;
}
