/**
 * Follow-up Question Types
 * Types related to follow-up question requests and responses
 */

/**
 * A single question in a follow-up question request
 */
export interface FollowupQuestion {
  /** Question index (0-based) */
  index: number;
  /** The question text */
  text: string;
  /** Available options for the user to choose from */
  options: Array<{
    value: string;
    description?: string;
  }>;
}

/**
 * Structured Follow-up Question Request Data
 * Rich context for follow-up question requests
 */
export interface FollowupQuestionRequestData {
  /** List of questions to ask */
  questions: FollowupQuestion[];
  /** Label for additional information input */
  additionalInfoLabel?: string;
}

/**
 * Structured Follow-up Question Response Data
 * User's response to follow-up question request
 */
export interface FollowupQuestionResponseData {
  /** Answers to the questions */
  answers: Array<{
    questionIndex: number;
    selectedOptionIndex: number; // -1 if custom input
    customInput?: string;
    answer: string;
  }>;
  /** Additional information provided by the user */
  additionalInfo?: string;
}
