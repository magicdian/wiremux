#import <Foundation/Foundation.h>
#import <SystemExtensions/SystemExtensions.h>

static NSString *const kWiremuxSerialDriverIdentifier =
    @"com.wiremux.DriverKitSerialPOC.WiremuxSerialDriver";

@interface WiremuxSystemExtensionDelegate
    : NSObject <OSSystemExtensionRequestDelegate>
@property(nonatomic) BOOL finished;
@property(nonatomic) int exitStatus;
@end

@implementation WiremuxSystemExtensionDelegate

- (void)requestNeedsUserApproval:(OSSystemExtensionRequest *)request {
    NSLog(@"wiremux DriverKit POC requires user approval for %@",
          request.identifier);
}

- (void)request:(OSSystemExtensionRequest *)request
    didFinishWithResult:(OSSystemExtensionRequestResult)result {
    NSLog(@"wiremux DriverKit POC activation finished for %@ with result %ld",
          request.identifier, (long)result);
    self.exitStatus = 0;
    self.finished = YES;
}

- (OSSystemExtensionReplacementAction)request:
        (OSSystemExtensionRequest *)request
                  actionForReplacingExtension:
                      (OSSystemExtensionProperties *)existing
                                 withExtension:
                                     (OSSystemExtensionProperties *)extension {
    NSLog(@"wiremux DriverKit POC replacing %@ %@ with %@ %@",
          existing.bundleIdentifier, existing.bundleVersion,
          extension.bundleIdentifier, extension.bundleVersion);
    return OSSystemExtensionReplacementActionReplace;
}

- (void)request:(OSSystemExtensionRequest *)request
    didFailWithError:(NSError *)error {
    NSLog(@"wiremux DriverKit POC activation failed for %@: %@",
          request.identifier, error);
    self.exitStatus = 1;
    self.finished = YES;
}

@end

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        (void)argc;
        (void)argv;

        NSLog(@"wiremux DriverKit serial POC host app");
        NSLog(@"embedded extension identifier: %@",
              kWiremuxSerialDriverIdentifier);

        NSDictionary<NSString *, NSString *> *environment =
            NSProcessInfo.processInfo.environment;
        if (![environment[@"WIREMUX_DRIVERKIT_ACTIVATE"]
                isEqualToString:@"1"]) {
            NSLog(@"set WIREMUX_DRIVERKIT_ACTIVATE=1 to submit an activation "
                  "request");
            return 0;
        }

        if (@available(macOS 10.15, *)) {
            WiremuxSystemExtensionDelegate *delegate =
                [WiremuxSystemExtensionDelegate new];
            OSSystemExtensionRequest *request =
                [OSSystemExtensionRequest
                    activationRequestForExtension:
                        kWiremuxSerialDriverIdentifier
                                           queue:dispatch_get_main_queue()];
            request.delegate = delegate;
            [OSSystemExtensionManager.sharedManager submitRequest:request];

            NSDate *deadline =
                [NSDate dateWithTimeIntervalSinceNow:30.0];
            while (!delegate.finished &&
                   [deadline timeIntervalSinceNow] > 0.0) {
                [NSRunLoop.currentRunLoop
                    runMode:NSDefaultRunLoopMode
                 beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
            }

            if (!delegate.finished) {
                NSLog(@"wiremux DriverKit POC activation timed out");
                return 2;
            }

            return delegate.exitStatus;
        }

        NSLog(@"OSSystemExtensionManager is unavailable on this macOS version");
        return 1;
    }
}
