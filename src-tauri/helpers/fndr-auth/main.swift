import Foundation
import LocalAuthentication

let reason = CommandLine.arguments.dropFirst().joined(separator: " ")
let context = LAContext()
var availabilityError: NSError?

guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &availabilityError) else {
    print("{\"type\":\"unavailable\"}")
    exit(EXIT_SUCCESS)
}

let semaphore = DispatchSemaphore(value: 0)
context.evaluatePolicy(
    .deviceOwnerAuthentication,
    localizedReason: reason.isEmpty ? "Unlock FNDR" : reason,
) { success, error in
    if success {
        print("{\"type\":\"authenticated\"}")
    } else if let error = error as? LAError, error.code == .userCancel || error.code == .appCancel || error.code == .systemCancel {
        print("{\"type\":\"cancelled\"}")
    } else {
        print("{\"type\":\"failed\"}")
    }
    semaphore.signal()
}
semaphore.wait()
