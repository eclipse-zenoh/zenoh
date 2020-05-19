pipeline {
  agent { label 'UbuntuVM' }
  parameters {
    gitParameter name: 'TAG', 
                 type: 'PT_TAG',
                 defaultValue: 'master'
  }

  stages {
    stage('Checkout Git TAG') {
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/atolab/eclipse-zenoh.git']]
                ])
      }
    }
    stage('Setup opam dependencies') {
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        OPAMJOBS=1 opam config report
        OPAMJOBS=1 opam install depext conf-libev
        OPAMJOBS=1 opam depext -yt
        OPAMJOBS=1 opam install -t . --deps-only
        '''
      }
    }
    stage('Build') {
      steps {
        sh '''
        opam exec -- dune build @all
        '''
      }
    }
    stage('Tests') {
      steps {
        sh '''
        opam exec -- dune runtest
        '''
      }
    }
    stage('Package') {
      steps {
        sh '''
        cp -r _build/default/install eclipse-zenoh
        tar czvf eclipse-zenoh-${TAG}-Ubuntu-20.04-x64.tgz eclipse-zenoh/*/*.*
        '''
      }
    }
    // stage('Docker build') {
    //   steps {
    //     sh '''
    //     docker build -t eclipse/zenoh:${TAG} .
    //     '''
    //   }
    // }
    // stage('Docker publish') {
    //   steps {
    //     withCredentials([usernamePassword(credentialsId: 'jenkins-docker-hub-creds',
    //         passwordVariable: 'DOCKER_HUB_CREDS_PSW', usernameVariable: 'DOCKER_HUB_CREDS_USR')])
    //     {
    //       sh '''
    //       echo "Login into docker as ${DOCKER_HUB_CREDS_USR}"
    //       docker login -u ${DOCKER_HUB_CREDS_USR} -p ${DOCKER_HUB_CREDS_PSW}
    //       docker push eclipse/zenoh:${TAG} .
    //       docker logout
    //       '''
    //     }
    //   }
    // }
    stage('Deploy to to download.eclipse.org') {
      steps {
        sshagent ( ['projects-storage.eclipse.org-bot-ssh']) {
          sh '''
          ssh genie.zenoh@projects-storage.eclipse.org mkdir -p /home/data/httpd/download.eclipse.org/zenoh/zenoh/${TAG}
          scp eclipse-zenoh-${TAG}-Ubuntu-20.04-x64.tgz  /home/data/httpd/download.eclipse.org/zenoh/zenoh/${TAG}/
          '''
        }
      }
    }
  }

  post {
    success {
        archiveArtifacts artifacts: 'eclipse-zenoh-${TAG}-Ubuntu-20.04-x64.tgz', fingerprint: true
    }
  }
}
