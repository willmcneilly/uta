export const meta = {
  name: 'review-round',
  description: 'One independent review of a Uta PR by a fresh pr-reviewer agent',
  whenToUse: 'From the work-ticket review loop after opening or updating a PR, or when Will asks for a review. Args: {pr, round?, commit?, dryRun?}',
  phases: [{ title: 'Review', detail: 'a fresh pr-reviewer checks the PR and posts its review' }],
}

// The reviewer's prompt is built here from checked values, never from text the
// implementer writes, so nothing about the implementation reaches the reviewer.
const input = args ?? {}
if (typeof input !== 'object' || Array.isArray(input)) {
  throw new Error('review-round takes {pr, round?, commit?, dryRun?}')
}
const known = ['pr', 'round', 'commit', 'dryRun']
const unknown = Object.keys(input).filter(k => !known.includes(k))
if (unknown.length) throw new Error(`review-round doesn't take: ${unknown.join(', ')}`)

const { pr, round = 1, commit, dryRun = false } = input
if (!Number.isInteger(pr) || pr < 1) throw new Error('pr must be a PR number')
if (!Number.isInteger(round) || round < 1 || round > 5) throw new Error('round must be 1 to 5')
if (commit !== undefined && !/^[0-9a-f]{7,40}$/.test(commit)) throw new Error('commit must be a commit SHA')
if (typeof dryRun !== 'boolean') throw new Error('dryRun must be true or false')

const finding = {
  type: 'object',
  properties: {
    severity: { enum: ['blocking', 'should_fix', 'nit'] },
    summary: { type: 'string' },
    file: { type: 'string' },
    line: { type: 'integer' },
  },
  required: ['severity', 'summary'],
}

const VERDICT = {
  type: 'object',
  properties: {
    verdict: { enum: ['ready', 'changes_needed'] },
    summary: { type: 'string', description: 'Two or three plain sentences for Will' },
    findings: { type: 'array', items: finding, description: 'Open findings only' },
    earlier_findings: {
      type: 'array',
      description: 'Re-reviews: every finding from earlier rounds and what happened to it',
      items: {
        type: 'object',
        properties: {
          summary: { type: 'string' },
          outcome: { enum: ['resolved', 'disagreement_accepted', 'still_open'] },
          note: { type: 'string' },
        },
        required: ['summary', 'outcome'],
      },
    },
    criteria: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          criterion: { type: 'string' },
          verified: { type: 'boolean' },
          how: { type: 'string' },
        },
        required: ['criterion', 'verified', 'how'],
      },
    },
    for_will: {
      type: 'array',
      description: 'Manual checks only Will can do, rated by how much they are worth',
      items: {
        type: 'object',
        properties: {
          utility: { enum: ['high', 'low'] },
          check: { type: 'string', description: 'Steps, and what he should hear or see' },
          why: { type: 'string' },
        },
        required: ['utility', 'check', 'why'],
      },
    },
    golden_changed: { type: 'boolean', description: 'A golden WAV changed, which Will must approve' },
    checks_green: { type: 'boolean', description: 'Every CI check run locally passed' },
    review_url: { type: 'string' },
    body: { type: 'string', description: 'Dry runs: the full review body' },
  },
  required: ['verdict', 'summary', 'findings', 'criteria', 'for_will', 'golden_changed', 'checks_green', 'review_url'],
}

let prompt = `Review PR #${pr}.`
if (round > 1) prompt += ` This is re-review round ${round}.`
if (commit) prompt += ` Review it as it was at commit ${commit}.`
if (dryRun) prompt += ' Dry run: post nothing to GitHub or Notion, and put the review body in the result.'

phase('Review')
const result = await agent(prompt, {
  agentType: 'pr-reviewer',
  schema: VERDICT,
  label: `PR #${pr}, round ${round}`,
})
if (!result) throw new Error(`The reviewer for PR #${pr} didn't finish`)
return result
